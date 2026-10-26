//! Shared severity vocabulary for verdicts, advisories and case creation.

use serde::{Deserialize, Serialize};

/// Ordered severity, lowest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational or low impact.
    Low,
    /// Moderate impact.
    Medium,
    /// High impact; triggers response handoff.
    High,
    /// Critical impact; highest response priority.
    Critical,
}

impl Severity {
    /// Parses the lowercase or mixed-case wire spelling.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" | "moderate" => Some(Self::Medium),
            "high" => Some(Self::High),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }

    /// Stable lowercase spelling used on the wire and in storage.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    /// 1-4 rank used for thresholds and TheHive case severity.
    pub fn rank(self) -> u8 {
        match self {
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
            Self::Critical => 4,
        }
    }

    /// Maps a CVSS base score to a severity band (NVD's own bands).
    pub fn from_cvss(score: f64) -> Self {
        match score {
            s if s >= 9.0 => Self::Critical,
            s if s >= 7.0 => Self::High,
            s if s >= 4.0 => Self::Medium,
            _ => Self::Low,
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::Severity;

    #[test]
    fn parse_and_rank_agree() {
        for (text, rank) in [("low", 1), ("Medium", 2), (" high ", 3), ("CRITICAL", 4)] {
            let sev = Severity::parse(text).expect(text);
            assert_eq!(sev.rank(), rank);
        }
        assert!(Severity::parse("sev1").is_none());
    }

    #[test]
    fn cvss_bands_match_nvd() {
        assert_eq!(Severity::from_cvss(9.8), Severity::Critical);
        assert_eq!(Severity::from_cvss(7.5), Severity::High);
        assert_eq!(Severity::from_cvss(5.0), Severity::Medium);
        assert_eq!(Severity::from_cvss(0.0), Severity::Low);
    }
}
