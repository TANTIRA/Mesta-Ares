//! CVSS v3.x scoring per the official FIRST specification.
//!
//! `v30` toggles the two v3.0-only differences: the modified-impact
//! environmental sub-equation and v3.0's `RC`/`RL` value sets being identical
//! (only the environmental formula differs in practice).

use std::collections::HashMap;

use crate::{split_vector, CvssError, CvssScore};

/// Required metric value, or `MissingMetric(key)` naming the absent metric.
fn required_metric<'a>(
    map: &'a HashMap<String, String>,
    key: &'static str,
) -> Result<&'a str, CvssError> {
    map.get(key)
        .map(String::as_str)
        .ok_or(CvssError::MissingMetric(key))
}

/// Missing optional metrics default to "X" (= base / no-op).
fn opt<'a>(map: &'a HashMap<String, String>, key: &str) -> &'a str {
    map.get(key).map(String::as_str).unwrap_or("X")
}

/// Modified-metric resolution: `M<key>` overrides `key` unless "X".
fn m<'a>(map: &'a HashMap<String, String>, key: &str) -> &'a str {
    let mk = format!("M{key}");
    match map.get(&mk).map(String::as_str) {
        Some(v) if v != "X" => v,
        _ => map.get(key).map(String::as_str).unwrap_or("X"),
    }
}

/// The spec's `Roundup` — smallest 0.1-aligned number ≥ input, computed to
/// avoid floating-point edge cases.
fn roundup(x: f64) -> f64 {
    let i = (x * 100_000.0).round() as i64;
    if i % 10_000 == 0 {
        i as f64 / 100_000.0
    } else {
        ((i / 10_000) + 1) as f64 / 10.0
    }
}

fn cia(v: &str) -> f64 {
    match v {
        "N" => 0.0,
        "L" => 0.22,
        "H" => 0.56,
        _ => 0.0,
    }
}

fn av(v: &str) -> f64 {
    match v {
        "N" => 0.85,
        "A" => 0.62,
        "L" => 0.55,
        "P" => 0.2,
        _ => 0.0,
    }
}

fn ac(v: &str) -> f64 {
    match v {
        "L" => 0.77,
        "H" => 0.44,
        _ => 0.0,
    }
}

fn pr(v: &str, scope_changed: bool) -> f64 {
    match (v, scope_changed) {
        ("N", _) => 0.85,
        ("L", false) => 0.62,
        ("L", true) => 0.68,
        ("H", false) => 0.27,
        ("H", true) => 0.5,
        _ => 0.0,
    }
}

fn ui(v: &str) -> f64 {
    match v {
        "N" => 0.85,
        "R" => 0.62,
        _ => 0.0,
    }
}

fn cr(v: &str) -> f64 {
    match v {
        "H" => 1.5,
        "M" => 1.0,
        "L" => 0.5,
        _ => 1.0,
    }
}

const VALID: &[(&str, &[&str])] = &[
    ("AV", &["N", "A", "L", "P"]),
    ("AC", &["L", "H"]),
    ("PR", &["N", "L", "H"]),
    ("UI", &["N", "R"]),
    ("S", &["U", "C"]),
    ("C", &["H", "L", "N"]),
    ("I", &["H", "L", "N"]),
    ("A", &["H", "L", "N"]),
    ("E", &["X", "H", "F", "P", "U"]),
    ("RL", &["X", "O", "T", "W", "U"]),
    ("RC", &["X", "U", "R", "C"]),
    ("CR", &["X", "H", "M", "L"]),
    ("IR", &["X", "H", "M", "L"]),
    ("AR", &["X", "H", "M", "L"]),
    ("MAV", &["X", "N", "A", "L", "P"]),
    ("MAC", &["X", "L", "H"]),
    ("MPR", &["X", "N", "L", "H"]),
    ("MUI", &["X", "N", "R"]),
    ("MS", &["X", "U", "C"]),
    ("MC", &["X", "H", "L", "N"]),
    ("MI", &["X", "H", "L", "N"]),
    ("MA", &["X", "H", "L", "N"]),
];

/// Score a `CVSS:3.x` vector. `v30` selects the v3.0 environmental formula.
pub(crate) fn score(vector: &str, v30: bool) -> Result<CvssScore, CvssError> {
    let (_, pairs) = split_vector(vector)?;
    let mut map = HashMap::new();
    for (k, v) in pairs {
        let Some(valid) = VALID.iter().find(|(name, _)| *name == k) else {
            return Err(CvssError::InvalidToken(k.to_string()));
        };
        if !valid.1.contains(&v) {
            return Err(CvssError::InvalidValue(k.to_string(), v.to_string()));
        }
        if map.insert(k.to_string(), v.to_string()).is_some() {
            return Err(CvssError::Duplicate(k.to_string()));
        }
    }
    for req in ["AV", "AC", "PR", "UI", "S", "C", "I", "A"] {
        required_metric(&map, req)?;
    }

    let scope_changed = required_metric(&map, "S")? == "C";
    let isc_base = (1.0 - cia(required_metric(&map, "C")?))
        * (1.0 - cia(required_metric(&map, "I")?))
        * (1.0 - cia(required_metric(&map, "A")?));
    let isc_base = 1.0 - isc_base;
    let impact = if scope_changed {
        7.52 * (isc_base - 0.029) - 3.25 * (isc_base - 0.02).powi(15)
    } else {
        6.42 * isc_base
    };
    let expl = 8.22
        * av(required_metric(&map, "AV")?)
        * ac(required_metric(&map, "AC")?)
        * pr(required_metric(&map, "PR")?, scope_changed)
        * ui(required_metric(&map, "UI")?);
    let base = if impact <= 0.0 {
        0.0
    } else if scope_changed {
        roundup((1.08 * (impact + expl)).min(10.0))
    } else {
        roundup((impact + expl).min(10.0))
    };

    // Temporal
    let e = match opt(&map, "E") {
        "H" | "X" => 1.0,
        "F" => 0.97,
        "P" => 0.94,
        "U" => 0.91,
        _ => 1.0,
    };
    let rl = match opt(&map, "RL") {
        "O" => 0.95,
        "T" => 0.96,
        "W" => 0.97,
        _ => 1.0,
    };
    let rc = match opt(&map, "RC") {
        "U" => 0.92,
        "R" => 0.96,
        _ => 1.0,
    };
    let temporal = roundup(base * e * rl * rc);

    // Environmental — skip when no env metrics were supplied.
    let has_env = map
        .keys()
        .any(|k| k.starts_with('M') || matches!(k.as_str(), "CR" | "IR" | "AR"));
    let final_score = if has_env {
        let m_scope_changed = m(&map, "S") == "C";
        let miss = (1.0
            - (1.0 - cr(opt(&map, "CR")) * cia(m(&map, "C")))
                * (1.0 - cr(opt(&map, "IR")) * cia(m(&map, "I")))
                * (1.0 - cr(opt(&map, "AR")) * cia(m(&map, "A"))))
        .min(0.915);
        let m_impact = if m_scope_changed {
            if v30 {
                7.52 * (miss - 0.029) - 3.25 * (miss - 0.02).powi(15)
            } else {
                7.52 * (miss - 0.029) - 3.25 * (miss * 0.9731 - 0.02).powi(13)
            }
        } else {
            6.42 * miss
        };
        let m_expl = 8.22
            * av(m(&map, "AV"))
            * ac(m(&map, "AC"))
            * pr(m(&map, "PR"), m_scope_changed)
            * ui(m(&map, "UI"));
        if m_impact <= 0.0 {
            0.0
        } else {
            let pre = if m_scope_changed {
                roundup((1.08 * (m_impact + m_expl)).min(10.0))
            } else {
                roundup((m_impact + m_expl).min(10.0))
            };
            roundup(pre * e * rl * rc)
        }
    } else {
        temporal
    };

    Ok(CvssScore {
        version: if v30 { "3.0" } else { "3.1" },
        vector: vector.to_string(),
        score: final_score,
        severity: crate::severity_of(final_score),
        nomenclature: String::new(),
        metrics: map,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_vector;

    #[test]
    fn known_base_scores() {
        let cases = [
            ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H", 9.8),
            ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H", 10.0),
            ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:L/I:L/A:L", 7.3),
            ("CVSS:3.1/AV:L/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H", 8.4),
            ("CVSS:3.1/AV:N/AC:H/PR:N/UI:N/S:U/C:N/I:N/A:N", 0.0),
            ("CVSS:3.1/AV:N/AC:L/PR:L/UI:N/S:U/C:L/I:N/A:N", 4.3),
        ];
        for (v, want) in cases {
            let got = parse_vector(v).unwrap();
            assert_eq!(got.score, want, "{v}");
        }
    }

    #[test]
    fn temporal_applies() {
        let got =
            parse_vector("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H/E:P/RL:O/RC:R").unwrap();
        assert_eq!(got.score, 8.5);
    }

    #[test]
    fn environmental_overrides() {
        let got = parse_vector(
            "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H/CR:H/IR:H/AR:H/MAV:A/MAC:H/MPR:H/MUI:R/MS:U/MC:N/MI:N/MA:N",
        )
        .unwrap();
        // MISS = 1-(1-1.5*.56)^3 -> miss capped .915; impacts all None -> env 0? no:
        // MC/MI/MA = N -> miss = 0 -> m_impact = 0 -> score 0.
        assert_eq!(got.score, 0.0);
    }

    #[test]
    fn rejects_malformed() {
        assert!(parse_vector("CVSS:3.1/AV:X/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H").is_err());
        assert!(parse_vector("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/C:H/I:H/A:H").is_err());
        assert!(parse_vector("CVSS:3.1/AV:N/AV:A/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H").is_err());
        assert!(parse_vector("not-a-vector").is_err());
    }

    #[test]
    fn roundup_edges() {
        assert_eq!(roundup(4.02), 4.1);
        assert_eq!(roundup(4.0), 4.0);
        assert_eq!(roundup(9.99), 10.0);
    }
}
