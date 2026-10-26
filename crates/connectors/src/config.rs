//! Environment-driven connector configuration.

use crate::cortex::CortexClient;
use crate::http::{env, Auth};
use crate::misp::MispClient;
use crate::nvd::NvdClient;
use crate::opensearch::OpensearchClient;
use crate::shuffle::ShuffleClient;
use crate::thehive::ThehiveClient;
use crate::wazuh::WazuhClient;

// ---------------------------------------------------------------------------
// Env-driven config
// ---------------------------------------------------------------------------

/// Connection settings for every connector, read from the environment.
#[derive(Debug, Clone, Default)]
pub struct ConnectorConfig {
    pub opensearch_url: Option<String>,
    pub opensearch_user: Option<String>,
    pub opensearch_pass: Option<String>,
    pub wazuh_api_url: Option<String>,
    pub wazuh_api_user: Option<String>,
    pub wazuh_api_pass: Option<String>,
    pub thehive_url: Option<String>,
    pub thehive_api_key: Option<String>,
    pub cortex_url: Option<String>,
    pub cortex_api_key: Option<String>,
    pub misp_url: Option<String>,
    pub misp_api_key: Option<String>,
    pub shuffle_url: Option<String>,
    pub shuffle_api_key: Option<String>,
    pub nvd_api_key: Option<String>,
}

impl ConnectorConfig {
    /// Read `OPENSEARCH_*`, `WAZUH_API_*`, `THEHIVE_*`, `CORTEX_*`, `MISP_*`,
    /// and `SHUFFLE_*` env vars. Missing vars become `None`.
    pub fn from_env() -> Self {
        Self {
            opensearch_url: env("OPENSEARCH_URL"),
            opensearch_user: env("OPENSEARCH_USER"),
            opensearch_pass: env("OPENSEARCH_PASS"),
            wazuh_api_url: env("WAZUH_API_URL"),
            wazuh_api_user: env("WAZUH_API_USER"),
            wazuh_api_pass: env("WAZUH_API_PASS"),
            thehive_url: env("THEHIVE_URL"),
            thehive_api_key: env("THEHIVE_API_KEY"),
            cortex_url: env("CORTEX_URL"),
            cortex_api_key: env("CORTEX_API_KEY"),
            misp_url: env("MISP_URL"),
            misp_api_key: env("MISP_API_KEY"),
            shuffle_url: env("SHUFFLE_URL"),
            shuffle_api_key: env("SHUFFLE_API_KEY"),
            nvd_api_key: env("NVD_API_KEY"),
        }
    }

    /// Builds an OpenSearch client if `OPENSEARCH_URL` is set.
    pub fn build_opensearch_client(&self) -> Option<OpensearchClient> {
        let url = self.opensearch_url.clone()?;
        let auth = match (&self.opensearch_user, &self.opensearch_pass) {
            (Some(u), Some(p)) => Auth::Basic {
                user: u.clone(),
                pass: p.clone(),
            },
            _ => Auth::None,
        };
        Some(OpensearchClient::new(url, auth))
    }

    /// Wazuh client if `WAZUH_API_URL` is set.
    pub fn build_wazuh_client(&self) -> Option<WazuhClient> {
        let url = self.wazuh_api_url.clone()?;
        let auth = match (&self.wazuh_api_user, &self.wazuh_api_pass) {
            (Some(u), Some(p)) => Auth::Basic {
                user: u.clone(),
                pass: p.clone(),
            },
            _ => Auth::None,
        };
        Some(WazuhClient::new(url, auth))
    }

    /// TheHive client if `THEHIVE_URL` is set.
    pub fn build_thehive_client(&self) -> Option<ThehiveClient> {
        let url = self.thehive_url.clone()?;
        let auth = self
            .thehive_api_key
            .clone()
            .map(Auth::ApiKey)
            .unwrap_or(Auth::None);
        Some(ThehiveClient::new(url, auth))
    }

    /// Cortex client if `CORTEX_URL` is set.
    pub fn build_cortex_client(&self) -> Option<CortexClient> {
        let url = self.cortex_url.clone()?;
        let auth = self
            .cortex_api_key
            .clone()
            .map(Auth::ApiKey)
            .unwrap_or(Auth::None);
        Some(CortexClient::new(url, auth))
    }

    /// MISP client if `MISP_URL` is set.
    pub fn build_misp_client(&self) -> Option<MispClient> {
        let url = self.misp_url.clone()?;
        let auth = self
            .misp_api_key
            .clone()
            .map(Auth::ApiKey)
            .unwrap_or(Auth::None);
        Some(MispClient::new(url, auth))
    }

    /// Shuffle client if `SHUFFLE_URL` is set.
    pub fn build_shuffle_client(&self) -> Option<ShuffleClient> {
        let url = self.shuffle_url.clone()?;
        let auth = self
            .shuffle_api_key
            .clone()
            .map(Auth::ApiKey)
            .unwrap_or(Auth::None);
        Some(ShuffleClient::new(url, auth))
    }

    /// NVD client; always available, with the faster rate limit when
    /// `NVD_API_KEY` is set.
    pub fn build_nvd_client(&self) -> NvdClient {
        NvdClient::new(self.nvd_api_key.clone())
    }
}
