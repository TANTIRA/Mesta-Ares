//! The invocable-agent registry shared by the A2A and ACP endpoints.
//!
//! One table describes every agent the platform can be asked to run: its card
//! name, NATS subject, envelope kind, the `ActivityEvent.agent` value it emits,
//! and the skills advertised over both protocols.

use agent_protocol::envelope::EnvelopeKind;
use agent_protocol::{SUBJECT_DETECTION, SUBJECT_HUNT, SUBJECT_RESPOND, SUBJECT_TRIAGE};

/// Static definition of one A2A-addressable Ares agent.
pub(crate) struct AgentDef {
    /// Card name and `/a2a/{name}` path segment.
    pub(crate) name: &'static str,
    /// Card description.
    pub(crate) description: &'static str,
    /// NATS subject the agent consumes.
    pub(crate) subject: &'static str,
    /// Envelope kind that the orchestrator routes to `subject`.
    pub(crate) kind: EnvelopeKind,
    /// Value agents put in `ActivityEvent.agent` for this agent.
    pub(crate) activity_name: &'static str,
    /// `(id, name, description, tags, examples)` rows for the card's skills.
    pub(crate) skills: &'static [SkillDef],
}

/// One skill tuple in an [`AgentDef`].
type SkillDef = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

/// The four invocable Ares agents.
pub(crate) const AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "triage-agent",
        description: "Alert triage: IOC extraction, threat-intel enrichment and an LLM verdict (tp/fp/suspicious).",
        subject: SUBJECT_TRIAGE,
        kind: EnvelopeKind::Alert,
        activity_name: "triage",
        skills: &[
            ("ioc-extraction", "IOC extraction", "Extract IPs, domains, URLs, hashes and emails from alert JSON.", &["ioc", "parsing"], &["Extract indicators from this alert"]),
            ("enrichment", "IOC enrichment", "Enrich IOCs against threat-intel providers and compute a composite score.", &["enrichment", "threat-intel"], &["Score the indicators in this alert"]),
            ("verdict", "Alert verdict", "Issue a tp/fp/suspicious verdict with severity and rationale.", &["verdict", "llm"], &["Triage this Wazuh alert"]),
        ],
    },
    AgentDef {
        name: "response-agent",
        description: "Autonomous incident response: plans and executes tool-driven actions for a case.",
        subject: SUBJECT_RESPOND,
        kind: EnvelopeKind::Command,
        activity_name: "response",
        skills: &[
            ("containment", "Containment", "Plan and run containment actions (host isolation, blocklists, account disable).", &["response", "containment"], &["Contain the compromised host"]),
            ("eradication", "Eradication", "Plan and run eradication/recovery actions for an incident.", &["response", "eradication"], &["Remove the persistence mechanism"]),
            ("planning", "Response planning", "Build an ordered, policy-checked action plan for a case.", &["planning"], &["Draft a response plan for this case"]),
        ],
    },
    AgentDef {
        name: "detection-agent",
        description: "Detection-as-code: drafts, validates, backtests and deploys Sigma content.",
        subject: SUBJECT_DETECTION,
        kind: EnvelopeKind::Intel,
        activity_name: "detection",
        skills: &[
            ("sigma", "Sigma authoring", "Draft Sigma rules for uncovered threat patterns.", &["sigma", "rules"], &["Write a rule for lateral movement via SMB"]),
            ("detection-engineering", "Detection engineering", "Validate and promote detection content through the pipeline.", &["pipeline"], &["Turn this threat-intel report into a detection"]),
            ("backtesting", "Rule backtesting", "Backtest candidate rules over the last 7 days of alerts.", &["backtest", "lucene"], &["Backtest this Sigma rule"]),
        ],
    },
    AgentDef {
        name: "hunt-agent",
        description: "Threat hunting: runs ATT&CK-mapped hypotheses over alert data and opens findings.",
        subject: SUBJECT_HUNT,
        kind: EnvelopeKind::HuntFinding,
        activity_name: "hunt",
        skills: &[
            ("hypothesis", "Hunt hypotheses", "Execute the ATT&CK-mapped hypothesis set.", &["hunt", "hypothesis"], &["Hunt for persistence mechanisms"]),
            ("attack-mapping", "ATT&CK mapping", "Map hunt coverage and findings to MITRE ATT&CK techniques.", &["attack", "mitre"], &["Map recent activity to ATT&CK"]),
            ("querying", "Log querying", "Query the alert store for hunt evidence.", &["opensearch", "lucene"], &["Find beaconing in the last 24h"]),
        ],
    },
];

/// Looks up an agent definition by its `/a2a/{name}` segment.
pub(crate) fn find_agent(name: &str) -> Option<&'static AgentDef> {
    AGENTS.iter().find(|a| a.name == name)
}
