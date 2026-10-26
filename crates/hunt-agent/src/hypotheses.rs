//! ATT&CK-mapped hunt hypotheses rendered to OpenSearch queries.
//!
//! Each hypothesis is a static template: metadata plus a function producing
//! the OpenSearch query body. Hunts run against `wazuh-alerts-*` over the
//! last 24 hours.

use serde_json::{json, Value};

/// Hunt lookback window.
const LOOKBACK: &str = "now-24h";

/// A threat-hunt hypothesis mapped to a MITRE ATT&CK technique.
pub struct Hypothesis {
    /// Short hunt identifier.
    pub id: &'static str,
    /// ATT&CK technique id (e.g. `T1003.001`).
    pub technique: &'static str,
    /// Human-readable hunt name.
    pub name: &'static str,
    /// Why this hunt is worth running.
    pub description: &'static str,
    /// Builds the OpenSearch query body for this hunt.
    pub query: fn() -> Value,
}

/// Wraps a `query_string` Lucene clause in a time-bounded bool query.
fn lucene(q: &str) -> Value {
    json!({
        "size": 100,
        "track_total_hits": true,
        "query": {"bool": {
            "must": [{"query_string": {"query": q}}],
            "filter": [{"range": {"@timestamp": {"gte": LOOKBACK}}}]
        }},
        "_source": ["agent.name", "data.win.eventdata", "rule.description", "@timestamp"]
    })
}

/// All registered hunt hypotheses (≥6, ATT&CK-mapped).
pub fn all() -> Vec<Hypothesis> {
    vec![
        Hypothesis {
            id: "hunt-lsass-dump",
            technique: "T1003.001",
            name: "LSASS memory access",
            description: "Processes other than the usual suspects opening a handle to lsass.exe.",
            query: || {
                lucene(
                    r#"data.win.eventdata.targetImage:*lsass.exe* AND NOT data.win.eventdata.sourceImage:(*MsMpEng.exe* OR *wininit.exe* OR *csrss.exe*)"#,
                )
            },
        },
        Hypothesis {
            id: "hunt-suspicious-cmdline",
            technique: "T1059",
            name: "Suspicious command lines",
            description:
                "Encoded PowerShell, download cradles and LOLBin abuse in process command lines.",
            query: || {
                lucene(
                    r#"data.win.eventdata.commandLine:(*-enc* OR *-e * OR *IEX* OR *downloadstring* OR *certutil* OR *bitsadmin*)"#,
                )
            },
        },
        Hypothesis {
            id: "hunt-valid-accounts",
            technique: "T1078",
            name: "Valid account anomaly",
            description:
                "Logons from unusual sources or at odd hours indicating stolen credentials.",
            query: || {
                lucene(
                    r#"data.win.eventdata.logonType:(3 OR 10) AND data.win.eventdata.authenticationPackageName:NTLM"#,
                )
            },
        },
        Hypothesis {
            id: "hunt-exfil-over-c2",
            technique: "T1041",
            name: "Exfiltration over C2",
            description: "Large outbound transfers to rare destinations (sysmon event 3 rollup).",
            query: || {
                lucene(
                    r#"rule.groups:network AND data.win.eventdata.destinationIsIpv6:false AND NOT data.win.eventdata.destinationIp:(10.* OR 192.168.* OR 172.16.*)"#,
                )
            },
        },
        Hypothesis {
            id: "hunt-runkey-persistence",
            technique: "T1547.001",
            name: "Run-key persistence",
            description: "Registry Run/RunOnce key writes establishing boot persistence.",
            query: || {
                lucene(
                    r#"data.win.eventdata.targetObject:(*\\Run\\* OR *\\RunOnce\\*) AND data.win.eventdata.eventType:SetValue"#,
                )
            },
        },
        Hypothesis {
            id: "hunt-c2-beacons",
            technique: "T1071.001",
            name: "C2 beaconing",
            description: "Repeated periodic connections to the same external host and port.",
            query: || {
                lucene(
                    r#"data.win.eventdata.image:(*powershell* OR *rundll32* OR *regsvr32* OR *mshta*) AND data.win.eventdata.destinationIp:*"#,
                )
            },
        },
        Hypothesis {
            id: "hunt-kerberoast",
            technique: "T1558.003",
            name: "Kerberoasting",
            description:
                "RC4-encrypted TGS requests for service accounts (event 4769, ticket type 0x17).",
            query: || {
                lucene(
                    r#"data.win.system.eventID:4769 AND data.win.eventdata.ticketEncryptionType:0x17"#,
                )
            },
        },
    ]
}
