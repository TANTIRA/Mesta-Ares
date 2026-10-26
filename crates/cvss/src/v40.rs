//! CVSS v4.0 scoring — port of the official FIRST reference implementation
//! (cvss_score.js / macroVector / maxComposed / maxSeverity, BSD-2-Clause).
//!
//! The algorithm maps a vector to a 6-digit MacroVector, looks up its score
//! in the published table, then interpolates downward using severity
//! distances to the macro's highest-severity vectors.

use std::collections::HashMap;

use crate::{lookup_table::lookup, split_vector, CvssError, CvssScore};

const STEP: f64 = 0.1;

// Metric severity levels (index of each value).
fn levels(metric: &str) -> &'static [(&'static str, f64)] {
    match metric {
        "AV" => &[("N", 0.0), ("A", 0.1), ("L", 0.2), ("P", 0.3)],
        "PR" => &[("N", 0.0), ("L", 0.1), ("H", 0.2)],
        "UI" => &[("N", 0.0), ("P", 0.1), ("A", 0.2)],
        "AC" => &[("L", 0.0), ("H", 0.1)],
        "AT" => &[("N", 0.0), ("P", 0.1)],
        "VC" | "VI" | "VA" => &[("H", 0.0), ("L", 0.1), ("N", 0.2)],
        "SC" => &[("H", 0.1), ("L", 0.2), ("N", 0.3)],
        "SI" | "SA" => &[("S", 0.0), ("H", 0.1), ("L", 0.2), ("N", 0.3)],
        "CR" | "IR" | "AR" => &[("H", 0.0), ("M", 0.1), ("L", 0.2)],
        "E" => &[("U", 0.2), ("P", 0.1), ("A", 0.0)],
        _ => &[],
    }
}

fn level(metric: &str, value: &str) -> f64 {
    levels(metric)
        .iter()
        .find(|(v, _)| *v == value)
        .map(|(_, l)| *l)
        .unwrap_or(0.0)
}

/// The metric validity table from metrics.js.
const VALID: &[(&str, &[&str])] = &[
    ("AV", &["N", "A", "L", "P"]),
    ("AC", &["L", "H"]),
    ("AT", &["N", "P"]),
    ("PR", &["N", "L", "H"]),
    ("UI", &["N", "P", "A"]),
    ("VC", &["H", "L", "N"]),
    ("VI", &["H", "L", "N"]),
    ("VA", &["H", "L", "N"]),
    ("SC", &["H", "L", "N"]),
    ("SI", &["H", "L", "N"]),
    ("SA", &["H", "L", "N"]),
    ("E", &["X", "A", "P", "U"]),
    ("CR", &["X", "H", "M", "L"]),
    ("IR", &["X", "H", "M", "L"]),
    ("AR", &["X", "H", "M", "L"]),
    ("MAV", &["X", "N", "A", "L", "P"]),
    ("MAC", &["X", "L", "H"]),
    ("MAT", &["X", "N", "P"]),
    ("MPR", &["X", "N", "L", "H"]),
    ("MUI", &["X", "N", "P", "A"]),
    ("MVC", &["X", "H", "L", "N"]),
    ("MVI", &["X", "H", "L", "N"]),
    ("MVA", &["X", "H", "L", "N"]),
    ("MSC", &["X", "H", "L", "N"]),
    ("MSI", &["X", "S", "H", "L", "N"]),
    ("MSA", &["X", "S", "H", "L", "N"]),
    ("S", &["X", "N", "P"]),
    ("AU", &["X", "N", "Y"]),
    ("R", &["X", "A", "U", "I"]),
    ("V", &["X", "D", "C"]),
    ("RE", &["X", "L", "M", "H"]),
    ("U", &["X", "Clear", "Green", "Amber", "Red"]),
];

const REQUIRED: &[&str] = &[
    "AV", "AC", "AT", "PR", "UI", "VC", "VI", "VA", "SC", "SI", "SA",
];

/// `m()` from the reference implementation: applies X-defaults for E and
/// CR/IR/AR, then environmental M-overrides.
fn m<'a>(sel: &'a HashMap<String, String>, metric: &str) -> &'a str {
    let s = sel.get(metric).map(String::as_str).unwrap_or("X");
    match metric {
        "E" if s == "X" => return "A",
        "CR" | "IR" | "AR" if s == "X" => return "H",
        _ => {}
    }
    let modified = format!("M{metric}");
    if let Some(v) = sel.get(&modified).map(String::as_str) {
        if v != "X" {
            return v;
        }
    }
    s
}

/// Highest-severity vectors composing each macro level (maxComposed).
fn max_composed(eq: u8, digit: u8, eq6: u8) -> &'static [&'static str] {
    match (eq, digit, eq6) {
        (1, 0, _) => &["AV:N/PR:N/UI:N/"],
        (1, 1, _) => &["AV:A/PR:N/UI:N/", "AV:N/PR:L/UI:N/", "AV:N/PR:N/UI:P/"],
        (1, 2, _) => &["AV:P/PR:N/UI:N/", "AV:A/PR:L/UI:P/"],
        (2, 0, _) => &["AC:L/AT:N/"],
        (2, 1, _) => &["AC:H/AT:N/", "AC:L/AT:P/"],
        (3, 0, 0) => &["VC:H/VI:H/VA:H/CR:H/IR:H/AR:H/"],
        (3, 0, 1) => &[
            "VC:H/VI:H/VA:L/CR:M/IR:M/AR:H/",
            "VC:H/VI:H/VA:H/CR:M/IR:M/AR:M/",
        ],
        (3, 1, 0) => &[
            "VC:L/VI:H/VA:H/CR:H/IR:H/AR:H/",
            "VC:H/VI:L/VA:H/CR:H/IR:H/AR:H/",
        ],
        (3, 1, 1) => &[
            "VC:L/VI:H/VA:L/CR:H/IR:M/AR:H/",
            "VC:L/VI:H/VA:H/CR:H/IR:M/AR:M/",
            "VC:H/VI:L/VA:H/CR:M/IR:H/AR:M/",
            "VC:H/VI:L/VA:L/CR:M/IR:H/AR:H/",
            "VC:L/VI:L/VA:H/CR:H/IR:H/AR:M/",
        ],
        (3, 2, 1) => &["VC:L/VI:L/VA:L/CR:H/IR:H/AR:H/"],
        (4, 0, _) => &["SC:H/SI:S/SA:S/"],
        (4, 1, _) => &["SC:H/SI:H/SA:H/"],
        (4, 2, _) => &["SC:L/SI:L/SA:L/"],
        (5, 0, _) => &["E:A/"],
        (5, 1, _) => &["E:P/"],
        (5, 2, _) => &["E:U/"],
        _ => &[],
    }
}

/// maxSeverity table (× STEP).
fn max_severity(eq: u8, digit: u8, eq6: u8) -> f64 {
    match (eq, digit, eq6) {
        (1, 0, _) => 1,
        (1, 1, _) => 4,
        (1, 2, _) => 5,
        (2, 0, _) => 1,
        (2, 1, _) => 2,
        (3, 0, 0) => 7,
        (3, 0, 1) => 6,
        (3, 1, 0) => 8,
        (3, 1, 1) => 8,
        (3, 2, 1) => 10,
        (4, 0, _) => 6,
        (4, 1, _) => 5,
        (4, 2, _) => 4,
        (5, 0, _) => 1,
        (5, 1, _) => 1,
        (5, 2, _) => 1,
        _ => 1,
    }
    .into()
}

fn severity<'a>(metric: &'a str, mv_vec: &'a str) -> &'a str {
    mv_vec
        .split('/')
        .filter_map(|p| p.split_once(':'))
        .find(|(k, _)| *k == metric)
        .map(|(_, v)| v)
        .unwrap_or("X")
}

fn macro_vector(sel: &HashMap<String, String>) -> String {
    let eq1 = if m(sel, "AV") == "N" && m(sel, "PR") == "N" && m(sel, "UI") == "N" {
        0
    } else if (m(sel, "AV") == "N" || m(sel, "PR") == "N" || m(sel, "UI") == "N")
        && m(sel, "AV") != "P"
    {
        1
    } else {
        2
    };
    let eq2 = if m(sel, "AC") == "L" && m(sel, "AT") == "N" {
        0
    } else {
        1
    };
    let eq3 = if m(sel, "VC") == "H" && m(sel, "VI") == "H" {
        0
    } else if m(sel, "VC") == "H" || m(sel, "VI") == "H" || m(sel, "VA") == "H" {
        1
    } else {
        2
    };
    let eq4 = if m(sel, "MSI") == "S" || m(sel, "MSA") == "S" {
        0
    } else if m(sel, "SC") == "H" || m(sel, "SI") == "H" || m(sel, "SA") == "H" {
        1
    } else {
        2
    };
    let eq5 = match m(sel, "E") {
        "A" => 0,
        "P" => 1,
        _ => 2,
    };
    let eq6 = if (m(sel, "CR") == "H" && m(sel, "VC") == "H")
        || (m(sel, "IR") == "H" && m(sel, "VI") == "H")
        || (m(sel, "AR") == "H" && m(sel, "VA") == "H")
    {
        0
    } else {
        1
    };
    format!("{eq1}{eq2}{eq3}{eq4}{eq5}{eq6}")
}

/// Score a `CVSS:4.0` vector string.
pub(crate) fn score(vector: &str) -> Result<CvssScore, CvssError> {
    let (_, pairs) = split_vector(vector)?;
    let mut sel = HashMap::new();
    for (k, v) in pairs {
        let Some(valid) = VALID.iter().find(|(name, _)| *name == k) else {
            return Err(CvssError::InvalidToken(k.to_string()));
        };
        if !valid.1.contains(&v) {
            return Err(CvssError::InvalidValue(k.to_string(), v.to_string()));
        }
        if sel.insert(k.to_string(), v.to_string()).is_some() {
            return Err(CvssError::Duplicate(k.to_string()));
        }
    }
    for req in REQUIRED {
        if !sel.contains_key(*req) {
            return Err(CvssError::MissingMetric(match *req {
                "AV" => "AV",
                _ => "base metric",
            }));
        }
    }

    // No impact on the system => 0.0 shortcut.
    let score = if ["VC", "VI", "VA", "SC", "SI", "SA"]
        .iter()
        .all(|metric| m(&sel, metric) == "N")
    {
        0.0
    } else {
        compute(&sel)
    };

    let threat = sel.get("E").is_some_and(|v| v != "X");
    let env = sel.iter().any(|(k, v)| {
        v.as_str() != "X"
            && matches!(
                k.as_str(),
                "CR" | "IR"
                    | "AR"
                    | "MAV"
                    | "MAC"
                    | "MAT"
                    | "MPR"
                    | "MUI"
                    | "MVC"
                    | "MVI"
                    | "MVA"
                    | "MSC"
                    | "MSI"
                    | "MSA"
            )
    });
    let nomenclature = match (threat, env) {
        (true, true) => "CVSS-BTE",
        (true, false) => "CVSS-BT",
        (false, true) => "CVSS-BE",
        (false, false) => "CVSS-B",
    }
    .to_string();

    Ok(CvssScore {
        version: "4.0",
        vector: vector.to_string(),
        score,
        severity: crate::severity_of(score),
        nomenclature,
        metrics: sel,
    })
}

fn compute(sel: &HashMap<String, String>) -> f64 {
    let mv = macro_vector(sel);
    let digits: Vec<u8> = mv.bytes().map(|b| b - b'0').collect();
    let (eq1, eq2, eq3, eq4, eq5, eq6) = (
        digits[0], digits[1], digits[2], digits[3], digits[4], digits[5],
    );
    let Some(mut value) = lookup(&mv) else {
        return 0.0;
    };

    // Next-lower macrovector per EQ (eq3/eq6 are coupled).
    let lower = |e1: u8, e2: u8, e3: u8, e4: u8, e5: u8, e6: u8| {
        lookup(&format!("{e1}{e2}{e3}{e4}{e5}{e6}"))
    };
    let score_eq1 = lower(eq1 + 1, eq2, eq3, eq4, eq5, eq6);
    let score_eq2 = lower(eq1, eq2 + 1, eq3, eq4, eq5, eq6);
    let score_eq3eq6 = match (eq3, eq6) {
        (0, 0) => {
            let left = lower(eq1, eq2, eq3, eq4, eq5, eq6 + 1);
            let right = lower(eq1, eq2, eq3 + 1, eq4, eq5, eq6);
            match (left, right) {
                (Some(l), Some(r)) => Some(l.max(r)),
                (l, r) => l.or(r),
            }
        }
        (1, 0) => lower(eq1, eq2, eq3, eq4, eq5, eq6 + 1),
        (0, 1) | (1, 1) => lower(eq1, eq2, eq3 + 1, eq4, eq5, eq6),
        _ => lower(eq1, eq2, eq3 + 1, eq4, eq5, eq6 + 1),
    };
    let score_eq4 = lower(eq1, eq2, eq3, eq4 + 1, eq5, eq6);
    let score_eq5 = lower(eq1, eq2, eq3, eq4, eq5 + 1, eq6);

    // Highest-severity vectors composing this macrovector.
    let eq1_maxes = max_composed(1, eq1, eq6);
    let eq2_maxes = max_composed(2, eq2, eq6);
    let eq3eq6_maxes = max_composed(3, eq3, eq6);
    let eq4_maxes = max_composed(4, eq4, eq6);
    let eq5_maxes = max_composed(5, eq5, eq6);

    let mut max_vectors = Vec::new();
    for a in eq1_maxes {
        for b in eq2_maxes {
            for c in eq3eq6_maxes {
                for d in eq4_maxes {
                    for e in eq5_maxes {
                        max_vectors.push(format!("{a}{b}{c}{d}{e}"));
                    }
                }
            }
        }
    }

    // First max vector whose every severity distance is >= the scored vector.
    let mut distances = None;
    for max_vec in &max_vectors {
        let d =
            |metric: &str| level(metric, m(sel, metric)) - level(metric, severity(metric, max_vec));
        let set = [
            d("AV"),
            d("PR"),
            d("UI"),
            d("AC"),
            d("AT"),
            d("VC"),
            d("VI"),
            d("VA"),
            d("SC"),
            d("SI"),
            d("SA"),
            d("CR"),
            d("IR"),
            d("AR"),
        ];
        if set.iter().all(|x| *x >= 0.0) {
            distances = Some(set);
            break;
        }
    }
    let Some(d) = distances else {
        return value;
    };

    let current_eq1 = d[0] + d[1] + d[2];
    let current_eq2 = d[3] + d[4];
    let current_eq3eq6 = d[5] + d[6] + d[7] + d[11] + d[12] + d[13];
    let current_eq4 = d[8] + d[9] + d[10];

    let mut n_existing_lower = 0usize;
    let mut normalized_sum = 0.0;
    let mut apply = |available: Option<f64>, current: f64, max_sev: f64| {
        if let Some(lower_score) = available {
            n_existing_lower += 1;
            let available_distance = value - lower_score;
            let pct = current / (max_sev * STEP);
            normalized_sum += available_distance * pct;
        }
    };
    apply(score_eq1, current_eq1, max_severity(1, eq1, eq6));
    apply(score_eq2, current_eq2, max_severity(2, eq2, eq6));
    apply(score_eq3eq6, current_eq3eq6, max_severity(3, eq3, eq6));
    apply(score_eq4, current_eq4, max_severity(4, eq4, eq6));
    apply(score_eq5, 0.0, max_severity(5, eq5, eq6));

    if n_existing_lower > 0 {
        value -= normalized_sum / n_existing_lower as f64;
    }
    (value.clamp(0.0, 10.0) * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use crate::parse_vector;

    #[test]
    fn known_v4_scores() {
        // Expected values verified against the official FIRST reference
        // implementation (cvss-v4-calculator).
        let cases = [
            ("CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:H/SI:H/SA:H", 10.0),
            (
                "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:H/SI:H/SA:H/E:P",
                9.3,
            ),
            (
                "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:H/SI:H/SA:H/E:U",
                9.1,
            ),
            (
                "CVSS:4.0/AV:P/AC:H/AT:P/PR:H/UI:A/VC:N/VI:N/VA:N/SC:N/SI:N/SA:N",
                0.0,
            ),
            (
                "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:L/VI:L/VA:N/SC:N/SI:N/SA:N",
                6.9,
            ),
            (
                "CVSS:4.0/AV:L/AC:H/AT:P/PR:H/UI:A/VC:L/VI:N/VA:N/SC:N/SI:N/SA:N",
                1.0,
            ),
            (
                "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:L/VA:N/SC:L/SI:N/SA:N",
                8.8,
            ),
            (
                "CVSS:4.0/AV:A/AC:L/AT:N/PR:L/UI:P/VC:H/VI:H/VA:L/SC:H/SI:H/SA:H/E:A/CR:M/IR:H/AR:M/MAV:N/MAC:H/MAT:N/MPR:L/MUI:P/MVC:H/MVI:L/MVA:N/MSC:H/MSI:S/MSA:S",
                7.6,
            ),
        ];
        for (v, want) in cases {
            let got = parse_vector(v).unwrap();
            assert_eq!(got.score, want, "{v}");
        }
    }

    #[test]
    fn nomenclature() {
        assert_eq!(
            parse_vector("CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:H/SI:H/SA:H")
                .unwrap()
                .nomenclature,
            "CVSS-B"
        );
        assert_eq!(
            parse_vector(
                "CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:H/SI:H/SA:H/E:P/CR:H"
            )
            .unwrap()
            .nomenclature,
            "CVSS-BTE"
        );
    }

    #[test]
    fn rejects_bad() {
        assert!(
            parse_vector("CVSS:4.0/AV:N/AC:L/PR:N/UI:N/VC:H/VI:H/VA:H/SC:H/SI:H/SA:H").is_err()
        );
        assert!(
            parse_vector("CVSS:4.0/AV:Z/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:H/SI:H/SA:H")
                .is_err()
        );
    }
}
