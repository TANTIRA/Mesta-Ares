//! CVSS vector parsing and scoring for v3.1 and v4.0.
//!
//! v3.1 implements the official base/temporal/environmental equations.
//! v4.0 implements the official FIRST macrovector + interpolation algorithm,
//! using the published lookup tables (see `lookup_table.rs`, ported from the
//! FIRST cvss-v4-calculator reference implementation, BSD-2-Clause).

mod lookup_table;
mod v31;
mod v40;

use std::collections::HashMap;

use serde::Serialize;
use thiserror::Error;

/// Errors from vector parsing or scoring.
#[derive(Debug, Error)]
pub enum CvssError {
    #[error("unsupported or missing CVSS version prefix")]
    UnsupportedVersion,
    #[error("malformed vector")]
    Malformed,
    #[error("missing required metric: {0}")]
    MissingMetric(&'static str),
    #[error("invalid value '{1}' for metric {0}")]
    InvalidValue(String, String),
    #[error("duplicate metric: {0}")]
    Duplicate(String),
    #[error("metric values must be single letters: {0}")]
    InvalidToken(String),
}

/// A scored CVSS vector.
#[derive(Debug, Clone, Serialize)]
pub struct CvssScore {
    /// "3.1" or "4.0".
    pub version: &'static str,
    /// Normalized vector string as parsed.
    pub vector: String,
    /// Base/temporal/environmental score, 0.0–10.0.
    pub score: f64,
    /// Qualitative severity band.
    pub severity: &'static str,
    /// v4.0 nomenclature (e.g. "CVSS-B", "CVSS-BTE"); empty for v3.1.
    pub nomenclature: String,
    /// Parsed metric -> value map (all explicitly set metrics; `X` entries
    /// are kept for v4.0 nomenclature computation).
    pub metrics: HashMap<String, String>,
}

/// FIRST qualitative severity bands (shared by v3.1 and v4.0).
pub fn severity_of(score: f64) -> &'static str {
    match score {
        s if s <= 0.0 => "None",
        s if s < 4.0 => "Low",
        s if s < 7.0 => "Medium",
        s if s < 9.0 => "High",
        _ => "Critical",
    }
}

/// `(metric, value)` pairs parsed from a vector.
type MetricPairs<'a> = Vec<(&'a str, &'a str)>;

/// Split a `CVSS:x.y/AV:N/AC:L/...` vector into its metric map. Returns the
/// version prefix separately. Does not validate values.
fn split_vector(vector: &str) -> Result<(&str, MetricPairs<'_>), CvssError> {
    let mut parts = vector.split('/');
    let version = parts.next().ok_or(CvssError::Malformed)?;
    let mut metrics = Vec::new();
    for part in parts {
        let (k, v) = part.split_once(':').ok_or(CvssError::Malformed)?;
        if k.is_empty() || v.is_empty() {
            return Err(CvssError::Malformed);
        }
        metrics.push((k, v));
    }
    Ok((version, metrics))
}

/// Parse and score a CVSS v3.1 or v4.0 vector string.
pub fn parse_vector(vector: &str) -> Result<CvssScore, CvssError> {
    let vector = vector.trim();
    match split_vector(vector)?.0 {
        "CVSS:3.1" => v31::score(vector, false),
        "CVSS:3.0" => v31::score(vector, true),
        "CVSS:4.0" => v40::score(vector),
        _ => Err(CvssError::UnsupportedVersion),
    }
}
