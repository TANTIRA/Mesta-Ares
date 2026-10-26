//! Thin async HTTP clients for the security stack's external systems.
//!
//! Each client wraps one upstream API behind `new(base_url, auth)` plus a
//! handful of typed methods that return raw `serde_json::Value` or lightly
//! typed models. Sends retry transient failures (connect/timeout, 429,
//! 502/503/504) twice with full jitter under a 15s budget; caching and
//! semantic retries remain the caller's job.
//!
//! Layout: one module per upstream ([`opensearch`], [`thehive`], [`cortex`],
//! [`misp`], [`wazuh`], [`shuffle`], [`nvd`]), [`intel`] for the threat-intel
//! providers, [`http`] for the shared credential/transport helpers, and
//! [`config`] for env-driven construction.

mod config;
mod cortex;
mod http;
pub mod intel;
mod misp;
mod nvd;
mod opensearch;
mod shuffle;
mod thehive;
mod wazuh;

pub use config::ConnectorConfig;
pub use cortex::CortexClient;
pub use http::Auth;
pub use misp::{Event, MispAttribute, MispClient, MispTag, NewAttribute};
pub use nvd::{CvePage, CveRecord, CveReference, CvssMetric, NvdClient, NvdStatusError};
pub use opensearch::OpensearchClient;
pub use shuffle::ShuffleClient;
pub use thehive::{Case, NewCase, NewObservable, Observable, ThehiveClient};
pub use wazuh::WazuhClient;
