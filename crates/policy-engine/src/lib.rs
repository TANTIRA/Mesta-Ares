//! Deterministic, deny-by-default authorization for every agent tool call.
//!
//! `PolicyEngine` is the hard boundary between agent intent and the
//! side-effecting executors in `tool-server`. Every [`ToolCall`] is evaluated
//! against the compiled allowlist in `tools.yml` plus the immutable
//! `protected-assets.yml` boundary. The order of checks in
//! [`PolicyEngine::evaluate`] is load-bearing and must not be reordered.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_protocol::tool::{Decision, DecisionKind, ToolCall};
use anyhow::{Context, Result};
use chrono::Utc;
use dashmap::DashMap;
use regex::Regex;
use serde::Deserialize;
use tracing::info;

/// Arg keys inspected for protected-asset matching and cooldown bucketing.
const TARGET_KEYS: &[&str] = &[
    "host",
    "hostname",
    "host_id",
    "agent_id",
    "ip",
    "remote",
    "user",
    "username",
    "user_id",
    "sam_account_name",
    "target",
    "domain",
    "service",
    "fqdn",
];

/// Mutable per-evaluation context shared across calls.
///
/// `asset_classes` maps a host identifier (hostname, agent id, ...) to its
/// asset class for protected-asset checks. `counters` holds rolling
/// blast-radius counters keyed `"tool:hour_bucket"` plus cooldown timestamps.
pub struct EvalCtx {
    pub asset_classes: HashMap<String, String>,
    pub counters: Arc<DashMap<String, u32>>,
}

impl EvalCtx {
    /// Create an empty evaluation context.
    pub fn new() -> Self {
        Self {
            asset_classes: HashMap::new(),
            counters: Arc::new(DashMap::new()),
        }
    }
}

impl Default for EvalCtx {
    fn default() -> Self {
        Self::new()
    }
}

/// Tool impact class from `tools.yml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Impact {
    Read,
    Write,
    High,
}

/// Autonomy level from `tools.yml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Autonomy {
    Auto,
    Approval,
    Deny,
}

/// Rate ceiling for high-impact tools.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct BlastRadius {
    #[serde(default)]
    pub max_per_hour: Option<u32>,
    #[serde(default)]
    pub cooldown_minutes: Option<u32>,
}

/// One entry of the tool allowlist.
#[derive(Debug, Deserialize)]
pub struct ToolPolicy {
    pub impact: Impact,
    pub autonomy: Autonomy,
    #[serde(default)]
    pub args_schema: Option<serde_yaml::Value>,
    #[serde(default)]
    pub blast_radius: Option<BlastRadius>,
    #[serde(default)]
    pub forbidden_args: HashMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ToolsFile {
    #[allow(dead_code)]
    version: u32,
    tools: HashMap<String, ToolPolicy>,
}

#[derive(Debug, Deserialize)]
struct ProtectedFile {
    #[allow(dead_code)]
    version: u32,
    protected: ProtectedRaw,
}

#[derive(Debug, Deserialize)]
struct ProtectedRaw {
    #[serde(default)]
    host_classes: HashSet<String>,
    #[serde(default)]
    wazuh_agent_ids: HashSet<String>,
    #[serde(default)]
    users: UsersSection,
    #[serde(default)]
    networks: NetworksSection,
    #[serde(default, rename = "self")]
    self_: SelfSection,
}

#[derive(Debug, Default, Deserialize)]
struct UsersSection {
    #[serde(default)]
    patterns: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct NetworksSection {
    #[serde(default)]
    cidrs: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SelfSection {
    #[serde(default)]
    services: HashSet<String>,
}

/// A parsed IPv4/IPv6 CIDR with contains/overlap support.
#[derive(Debug, Clone, Copy)]
enum Cidr {
    V4 { base: u32, bits: u8 },
    V6 { base: u128, bits: u8 },
}

fn mask4(bits: u8) -> u32 {
    if bits == 0 {
        0
    } else {
        u32::MAX << (32 - bits)
    }
}

fn mask6(bits: u8) -> u128 {
    if bits == 0 {
        0
    } else {
        u128::MAX << (128 - bits)
    }
}

impl Cidr {
    fn parse(s: &str) -> Option<Self> {
        let (addr, bits) = s.split_once('/')?;
        let bits: u8 = bits.trim().parse().ok()?;
        match addr.trim().parse::<IpAddr>().ok()? {
            IpAddr::V4(a) if bits <= 32 => Some(Self::V4 {
                base: u32::from(a) & mask4(bits),
                bits,
            }),
            IpAddr::V6(a) if bits <= 128 => Some(Self::V6 {
                base: u128::from(a) & mask6(bits),
                bits,
            }),
            _ => None,
        }
    }

    fn contains(&self, ip: IpAddr) -> bool {
        match (self, ip) {
            (Self::V4 { base, bits }, IpAddr::V4(a)) => (u32::from(a) & mask4(*bits)) == *base,
            (Self::V6 { base, bits }, IpAddr::V6(a)) => (u128::from(a) & mask6(*bits)) == *base,
            _ => false,
        }
    }

    /// Inclusive `[start, end]` range tagged by address family.
    fn range(&self) -> (u8, u128, u128) {
        match *self {
            Self::V4 { base, bits } => {
                (4, base as u128, base as u128 + ((1u128 << (32 - bits)) - 1))
            }
            Self::V6 { base, bits } => (6, base, base + ((1u128 << (128 - bits)) - 1)),
        }
    }

    fn overlaps(&self, other: &Self) -> bool {
        let (f1, s1, e1) = self.range();
        let (f2, s2, e2) = other.range();
        f1 == f2 && s1 <= e2 && s2 <= e1
    }
}

/// Immutable protected-asset boundary compiled from `protected-assets.yml`.
struct Protected {
    host_classes: HashSet<String>,
    wazuh_agent_ids: HashSet<String>,
    user_patterns: Vec<Regex>,
    cidrs: Vec<Cidr>,
    self_services: HashSet<String>,
}

impl Protected {
    fn from_raw(raw: ProtectedRaw) -> Result<Self> {
        let user_patterns = raw
            .users
            .patterns
            .iter()
            .map(|p| Regex::new(p).with_context(|| format!("invalid protected user pattern `{p}`")))
            .collect::<Result<Vec<_>>>()?;
        let cidrs = raw
            .networks
            .cidrs
            .iter()
            .map(|c| {
                Cidr::parse(c).with_context(|| format!("invalid protected network CIDR `{c}`"))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            host_classes: raw.host_classes,
            wazuh_agent_ids: raw.wazuh_agent_ids,
            user_patterns,
            cidrs,
            self_services: raw.self_.services,
        })
    }
}

/// Allowlist entry with its pre-compiled JSON Schema validator.
struct CompiledTool {
    policy: ToolPolicy,
    validator: Option<jsonschema::Validator>,
}

/// Compiled policy set loaded once from a policy directory.
pub struct PolicyEngine {
    dir: PathBuf,
    tools: HashMap<String, CompiledTool>,
    protected: Protected,
}

impl PolicyEngine {
    /// Load `{dir}/tools.yml` and `{dir}/protected-assets.yml`, compiling JSON
    /// Schemas, user regexes, and CIDRs eagerly so `evaluate` stays cheap.
    pub fn load(dir: &Path) -> Result<Self> {
        let tools_text = std::fs::read_to_string(dir.join("tools.yml"))
            .with_context(|| format!("read {}/tools.yml", dir.display()))?;
        let parsed: ToolsFile = serde_yaml::from_str(&tools_text).context("parse tools.yml")?;

        let mut tools = HashMap::with_capacity(parsed.tools.len());
        for (name, policy) in parsed.tools {
            let validator = match &policy.args_schema {
                Some(schema) => {
                    let json_schema = serde_json::to_value(schema).with_context(|| {
                        format!("args_schema for `{name}` is not JSON-convertible")
                    })?;
                    Some(
                        jsonschema::validator_for(&json_schema)
                            .with_context(|| format!("invalid args_schema for `{name}`"))?,
                    )
                }
                None => None,
            };
            tools.insert(name, CompiledTool { policy, validator });
        }

        let prot_text = std::fs::read_to_string(dir.join("protected-assets.yml"))
            .with_context(|| format!("read {}/protected-assets.yml", dir.display()))?;
        let prot_file: ProtectedFile =
            serde_yaml::from_str(&prot_text).context("parse protected-assets.yml")?;
        let protected = Protected::from_raw(prot_file.protected)?;

        Ok(Self {
            dir: dir.to_path_buf(),
            tools,
            protected,
        })
    }

    /// Sorted names of every allowlisted tool (including `autonomy: deny`
    /// entries, which exist for documentation).
    pub fn tool_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.tools.keys().cloned().collect();
        v.sort();
        v
    }

    /// Evaluate a tool call. Checks run in a fixed order:
    /// kill switch → allowlist → autonomy → args_schema → forbidden_args →
    /// protected assets → blast radius → allow.
    pub async fn evaluate(&self, call: &ToolCall, ctx: &mut EvalCtx) -> Decision {
        let decision = self.evaluate_inner(call, ctx, false);
        info!(
            tool = %call.tool,
            actor = %call.actor,
            correlation_id = %call.correlation_id,
            decision = ?decision.kind,
            reason = %decision.reason,
            "policy decision"
        );
        decision
    }

    /// Evaluate a call a human already approved: the `autonomy: approval`
    /// gate is satisfied, every other check (kill switch, allowlist,
    /// args_schema, forbidden_args, protected assets, blast radius) still
    /// applies.
    pub async fn evaluate_approved(&self, call: &ToolCall, ctx: &mut EvalCtx) -> Decision {
        let decision = self.evaluate_inner(call, ctx, true);
        info!(
            tool = %call.tool,
            actor = %call.actor,
            correlation_id = %call.correlation_id,
            decision = ?decision.kind,
            reason = %decision.reason,
            "policy decision (approved call)"
        );
        decision
    }

    fn evaluate_inner(&self, call: &ToolCall, ctx: &mut EvalCtx, approved: bool) -> Decision {
        // (1) Kill switch: env flag or non-empty `{dir}/kill` file.
        if self.kill_switch_engaged() {
            return decide(DecisionKind::Deny, "kill switch engaged");
        }

        // (2) Deny-by-default allowlist.
        let Some(tool) = self.tools.get(&call.tool) else {
            return decide(
                DecisionKind::Deny,
                format!("tool `{}` not in allowlist", call.tool),
            );
        };

        // (3)–(4) Autonomy. A human approval satisfies this gate only.
        match tool.policy.autonomy {
            Autonomy::Deny => {
                return decide(
                    DecisionKind::Deny,
                    format!("tool `{}` is denied by policy", call.tool),
                )
            }
            Autonomy::Approval if !approved => {
                return decide(
                    DecisionKind::ApprovalRequired,
                    format!("tool `{}` requires human approval", call.tool),
                )
            }
            _ => {}
        }

        // (5) args_schema validation.
        if let Some(validator) = &tool.validator {
            if let Err(e) = validator.validate(&call.args) {
                return decide(
                    DecisionKind::Deny,
                    format!("args_schema violation for `{}`: {e}", call.tool),
                );
            }
        }

        // (6) forbidden_args: IP values must not be private/loopback/etc.
        if let Some(reason) = check_forbidden_args(&call.args, &tool.policy.forbidden_args) {
            return decide(DecisionKind::Deny, format!("forbidden arg: {reason}"));
        }

        // (7) Protected assets. Per protected-assets.yml, the boundary applies
        // to write/high-impact calls only; read-only lookups are untouched.
        if tool.policy.impact != Impact::Read {
            if let Some(reason) = self.check_protected(&call.args, ctx) {
                return decide(DecisionKind::Deny, format!("protected asset: {reason}"));
            }
        }

        // (8) Blast radius: hourly cap and per-target cooldown.
        if let Some(br) = tool.policy.blast_radius {
            let bucket = Utc::now().format("%Y%m%d%H");
            let key = format!("{}:{bucket}", call.tool);
            let count = {
                let mut entry = ctx.counters.entry(key).or_insert(0);
                *entry += 1;
                *entry
            };
            if let Some(max) = br.max_per_hour {
                if count > max {
                    return decide(
                        DecisionKind::Deny,
                        format!(
                            "blast radius exceeded for `{}`: {count} > {max} calls this hour",
                            call.tool
                        ),
                    );
                }
            }
            if let Some(cd) = br.cooldown_minutes {
                let now_min = (Utc::now().timestamp().max(0) as u64 / 60) as u32;
                let target = extract_targets(&call.args)
                    .into_iter()
                    .next()
                    .unwrap_or_default();
                let ck = format!("cooldown:{}:{target}", call.tool);
                if let Some(prev) = ctx.counters.get(&ck) {
                    if now_min.saturating_sub(*prev) < cd {
                        return decide(
                            DecisionKind::Deny,
                            format!("cooldown active for `{}` on `{target}` ({cd}m)", call.tool),
                        );
                    }
                }
                drop(ctx.counters.get(&ck));
                ctx.counters.insert(ck, now_min);
            }
        }

        // (9) Allow.
        decide(DecisionKind::Allow, "allowed")
    }

    /// Env flag or non-empty kill file engages the switch.
    fn kill_switch_engaged(&self) -> bool {
        if let Ok(v) = std::env::var("ARES_KILL_SWITCH") {
            if matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ) {
                return true;
            }
        }
        match std::fs::read_to_string(self.dir.join("kill")) {
            Ok(s) => !s.trim().is_empty(),
            Err(_) => false,
        }
    }

    /// Match call targets against the protected-asset boundary.
    fn check_protected(&self, args: &serde_json::Value, ctx: &EvalCtx) -> Option<String> {
        for t in extract_targets(args) {
            if self.protected.wazuh_agent_ids.contains(&t) {
                return Some(format!("wazuh agent id `{t}` is protected"));
            }
            if self.protected.self_services.contains(&t) {
                return Some(format!("`{t}` is a protected control-plane service"));
            }
            if let Some(class) = ctx.asset_classes.get(&t) {
                if self.protected.host_classes.contains(class) {
                    return Some(format!("target `{t}` has protected host class `{class}`"));
                }
            }
            if self.protected.user_patterns.iter().any(|r| r.is_match(&t)) {
                return Some(format!("target `{t}` matches a protected user pattern"));
            }
            if let Ok(ip) = t.parse::<IpAddr>() {
                if let Some(c) = self.protected.cidrs.iter().find(|c| c.contains(ip)) {
                    return Some(format!("target `{t}` is inside protected network {c:?}"));
                }
            } else if let Some(cidr) = Cidr::parse(&t) {
                if self.protected.cidrs.iter().any(|c| c.overlaps(&cidr)) {
                    return Some(format!("target `{t}` overlaps a protected network"));
                }
            }
        }
        None
    }
}

/// Collect string values of target-bearing args.
fn extract_targets(args: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(map) = args.as_object() {
        for (k, v) in map {
            if TARGET_KEYS.contains(&k.as_str()) {
                match v {
                    serde_json::Value::String(s) => out.push(s.clone()),
                    serde_json::Value::Array(a) => {
                        out.extend(a.iter().filter_map(|x| x.as_str()).map(|x| x.to_string()))
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

/// Enforce `forbidden_args`: named IP args must not be in a forbidden class.
fn check_forbidden_args(
    args: &serde_json::Value,
    spec: &HashMap<String, Vec<String>>,
) -> Option<String> {
    for (arg, classes) in spec {
        let Some(raw) = args.get(arg).and_then(|v| v.as_str()) else {
            continue;
        };
        let Ok(ip) = raw.parse::<IpAddr>() else {
            continue;
        };
        for class in classes {
            let hit = match class.as_str() {
                "RFC1918" => match ip {
                    IpAddr::V4(a) => a.is_private(),
                    IpAddr::V6(_) => false,
                },
                "loopback" => ip.is_loopback(),
                "multicast" => ip.is_multicast(),
                "link_local" => match ip {
                    IpAddr::V4(a) => a.is_link_local(),
                    IpAddr::V6(a) => a.is_unicast_link_local(),
                },
                "unspecified" => ip.is_unspecified(),
                "cgNAT" | "shared" => match ip {
                    IpAddr::V4(a) => a.octets()[0] == 100 && (a.octets()[1] & 0xC0) == 64,
                    IpAddr::V6(_) => false,
                },
                _ => false,
            };
            if hit {
                return Some(format!("`{arg}`={ip} is in forbidden class `{class}`"));
            }
        }
    }
    None
}

fn decide(kind: DecisionKind, reason: impl Into<String>) -> Decision {
    Decision {
        kind,
        reason: reason.into(),
        decided_at: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn policies_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../policies")
            .canonicalize()
            .expect("policies dir")
    }

    fn engine() -> PolicyEngine {
        PolicyEngine::load(&policies_dir()).expect("load policies")
    }

    fn call(tool: &str, args: serde_json::Value) -> ToolCall {
        ToolCall {
            id: Uuid::new_v4(),
            tool: tool.to_string(),
            args,
            actor: "test".to_string(),
            idempotency_key: String::new(),
            correlation_id: Uuid::new_v4(),
        }
    }

    #[tokio::test]
    async fn unknown_tool_is_denied() {
        let mut ctx = EvalCtx::new();
        let d = engine()
            .evaluate(&call("nope.missing", json!({})), &mut ctx)
            .await;
        assert!(matches!(&d.kind, DecisionKind::Deny));
        assert!(d.reason.contains("allowlist"));
    }

    #[tokio::test]
    async fn deny_listed_tool_is_denied() {
        let mut ctx = EvalCtx::new();
        let d = engine()
            .evaluate(&call("host.shutdown", json!({})), &mut ctx)
            .await;
        assert!(matches!(&d.kind, DecisionKind::Deny));
    }

    #[tokio::test]
    async fn schema_violation_is_denied() {
        let mut ctx = EvalCtx::new();
        // `duration_minutes` exceeds the schema maximum (10080).
        let d = engine()
            .evaluate(
                &call(
                    "blocklist.ip.add",
                    json!({"ip": "203.0.113.9", "duration_minutes": 99999}),
                ),
                &mut ctx,
            )
            .await;
        assert!(matches!(&d.kind, DecisionKind::Deny), "got {d:?}");
    }

    #[tokio::test]
    async fn rfc1918_ip_is_blocked() {
        let mut ctx = EvalCtx::new();
        let d = engine()
            .evaluate(
                &call("blocklist.ip.add", json!({"ip": "10.1.2.3"})),
                &mut ctx,
            )
            .await;
        assert!(matches!(&d.kind, DecisionKind::Deny));
        assert!(d.reason.contains("forbidden"));
    }

    #[tokio::test]
    async fn protected_user_is_denied() {
        let mut ctx = EvalCtx::new();
        let d = engine()
            .evaluate(
                &call("ad.user.disable", json!({"sam_account_name": "svc-agent"})),
                &mut ctx,
            )
            .await;
        assert!(matches!(&d.kind, DecisionKind::Deny), "got {d:?}");
        assert!(d.reason.contains("protected"));
    }

    #[tokio::test]
    async fn protected_host_class_is_denied() {
        let mut ctx = EvalCtx::new();
        ctx.asset_classes
            .insert("001".to_string(), "domain_controller".to_string());
        let d = engine()
            .evaluate(
                &call(
                    "wazuh.active_response.isolate_host",
                    json!({"agent_id": "001"}),
                ),
                &mut ctx,
            )
            .await;
        assert!(matches!(&d.kind, DecisionKind::Deny), "got {d:?}");
        assert!(d.reason.contains("protected"));
    }

    #[tokio::test]
    async fn blast_radius_cap_is_enforced() {
        let mut ctx = EvalCtx::new();
        let e = engine();
        // max_per_hour for isolate_host is 5; vary agent_id to dodge cooldown.
        for i in 1..=5u32 {
            let d = e
                .evaluate(
                    &call(
                        "wazuh.active_response.isolate_host",
                        json!({"agent_id": format!("{i:03}")}),
                    ),
                    &mut ctx,
                )
                .await;
            assert!(matches!(&d.kind, DecisionKind::Allow), "call {i}: {d:?}");
        }
        let d = e
            .evaluate(
                &call(
                    "wazuh.active_response.isolate_host",
                    json!({"agent_id": "099"}),
                ),
                &mut ctx,
            )
            .await;
        assert!(matches!(&d.kind, DecisionKind::Deny));
        assert!(d.reason.contains("blast radius"));
    }

    #[tokio::test]
    async fn cooldown_blocks_repeat_same_target() {
        let mut ctx = EvalCtx::new();
        let e = engine();
        let args = json!({"agent_id": "777"});
        let d1 = e
            .evaluate(
                &call("wazuh.active_response.isolate_host", args.clone()),
                &mut ctx,
            )
            .await;
        assert!(matches!(&d1.kind, DecisionKind::Allow), "{d1:?}");
        let d2 = e
            .evaluate(&call("wazuh.active_response.isolate_host", args), &mut ctx)
            .await;
        assert!(matches!(&d2.kind, DecisionKind::Deny));
        assert!(d2.reason.contains("cooldown"));
    }

    #[tokio::test]
    async fn allowlisted_tool_is_allowed() {
        let mut ctx = EvalCtx::new();
        let d = engine()
            .evaluate(
                &call("opensearch.query", json!({"index": "alerts-*"})),
                &mut ctx,
            )
            .await;
        assert!(matches!(&d.kind, DecisionKind::Allow), "got {d:?}");
    }

    /// MISP write tools are gated on human approval, not auto-run.
    #[tokio::test]
    async fn misp_writes_require_approval() {
        for tool in ["misp.attribute.add", "misp.ioc.block", "misp.sighting.add"] {
            let mut ctx = EvalCtx::new();
            let d = engine()
                .evaluate(&call(tool, json!({"event_id": "1"})), &mut ctx)
                .await;
            assert!(
                matches!(&d.kind, DecisionKind::ApprovalRequired),
                "{tool} should require approval, got {d:?}"
            );
        }
    }

    /// Approval satisfies autonomy only; schema checks still deny.
    #[tokio::test]
    async fn approved_call_skips_autonomy_but_keeps_schema() {
        let mut ctx = EvalCtx::new();
        let ok = engine()
            .evaluate_approved(
                &call("misp.sighting.add", json!({"value": "203.0.113.9"})),
                &mut ctx,
            )
            .await;
        assert!(matches!(&ok.kind, DecisionKind::Allow), "got {ok:?}");

        let bad = engine()
            .evaluate_approved(
                &call("misp.event.view", json!({"event_id": "not-a-number"})),
                &mut ctx,
            )
            .await;
        assert!(
            matches!(&bad.kind, DecisionKind::Deny),
            "schema must still deny, got {bad:?}"
        );
    }

    #[tokio::test]
    async fn kill_file_engages_switch() {
        let dir = std::env::temp_dir().join(format!("ares-policies-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["tools.yml", "protected-assets.yml"] {
            std::fs::copy(policies_dir().join(f), dir.join(f)).unwrap();
        }
        std::fs::write(dir.join("kill"), "engaged\n").unwrap();
        let e = PolicyEngine::load(&dir).unwrap();
        let mut ctx = EvalCtx::new();
        let d = e
            .evaluate(&call("opensearch.query", json!({})), &mut ctx)
            .await;
        assert!(matches!(&d.kind, DecisionKind::Deny));
        assert!(d.reason.contains("kill switch"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
