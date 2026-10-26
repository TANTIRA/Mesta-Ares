//! Shared YAML directory loader for the read-model endpoints.

use serde_json::Value;

/// Loads every `*.yml`/`*.yaml` file in `dir`, sorted by file name.
///
/// Unreadable files and parse failures are returned in the second element so
/// callers can report or log them instead of silently dropping content.
pub fn load(dir: &std::path::Path) -> (Vec<(String, Value)>, Vec<String>) {
    let mut docs = Vec::new();
    let mut errors = Vec::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            errors.push(format!("cannot read {}: {e}", dir.display()));
            return (docs, errors);
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yml") | Some("yaml")
        ) {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        match std::fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|text| serde_yaml_ng::from_str::<Value>(&text).map_err(Into::into))
        {
            Ok(doc) => docs.push((name, doc)),
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }

    docs.sort_by(|a, b| a.0.cmp(&b.0));
    (docs, errors)
}
