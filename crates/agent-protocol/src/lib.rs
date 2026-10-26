//! Shared contract types for the ARES agentic SIEM+SOAR platform.
//!
//! Every service in the workspace speaks the types defined here: message
//! envelopes, tool calls and policy decisions, agent handoff payloads, and
//! hash-chained audit events. NATS subject constants name the topics that
//! connect the services.

#![warn(missing_docs)]

pub mod activity;
pub mod audit;
pub mod envelope;
pub mod handoff;
pub mod kill;
pub mod severity;
pub mod tool;

/// NATS subject carrying raw alerts produced by the ingestion gateway.
pub const SUBJECT_ALERTS: &str = "ares.alerts";
/// NATS subject carrying triage results.
pub const SUBJECT_TRIAGE: &str = "ares.triage";
/// NATS subject carrying response actions.
pub const SUBJECT_RESPOND: &str = "ares.respond";
/// NATS subject carrying detection engineering output.
pub const SUBJECT_DETECTION: &str = "ares.detection";
/// NATS subject carrying threat-hunt findings.
pub const SUBJECT_HUNT: &str = "ares.hunt";
/// NATS subject carrying breach-and-attack-simulation results.
pub const SUBJECT_BAS: &str = "ares.bas";
/// NATS subject carrying vulnerability advisories from vuln-agent.
pub const SUBJECT_VULNS: &str = "ares.vulns";
/// Dead-letter subject for messages the orchestrator cannot route.
pub const SUBJECT_DLQ: &str = "ares.dlq";
