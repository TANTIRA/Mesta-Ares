//! MISP threat-intel client (events, attributes, warninglists, sightings).
use std::collections::HashMap;

use anyhow::{Context, Result};

use reqwest::{header, Client, RequestBuilder};

use serde::{Deserialize, Serialize};

use serde_json::Value;

use crate::http::{join, json_ok, segment, send_with_retry, Auth};

// ---------------------------------------------------------------------------
// MISP
// ---------------------------------------------------------------------------

/// A MISP event from `restSearch`; unknown fields land in `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    #[serde(rename = "id", deserialize_with = "de_string_or_num")]
    pub id: String,
    #[serde(default)]
    pub info: String,
    #[serde(default)]
    pub date: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// New attribute payload for `POST /attributes/add/{event_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAttribute {
    #[serde(rename = "type")]
    pub attr_type: String,
    pub category: String,
    pub value: String,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default, rename = "to_ids")]
    pub to_ids: bool,
}

fn de_string_or_num<'de, D>(d: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match Value::deserialize(d)? {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        _ => Ok(String::new()),
    }
}

fn de_opt_string_or_num<'de, D>(d: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match Value::deserialize(d)? {
        Value::String(s) => Some(s),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

/// Values from a `warninglists/checkValue` response that matched at least one
/// list. Shape: `{"<value>": [{"id": "10", "name": "..."}, ...]}`.
fn warninglist_matches(raw: &Value) -> Vec<String> {
    raw.as_object()
        .map(|m| {
            m.iter()
                .filter(|(_, hits)| hits.as_array().is_some_and(|a| !a.is_empty()))
                .map(|(v, _)| v.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// A MISP attribute from `POST /attributes/restSearch`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MispAttribute {
    /// Attribute id (numeric on the wire).
    #[serde(rename = "id", default, deserialize_with = "de_string_or_num")]
    pub id: String,
    /// Owning event id.
    #[serde(
        rename = "event_id",
        default,
        deserialize_with = "de_opt_string_or_num"
    )]
    pub event_id: Option<String>,
    /// Attribute type (`ip-dst`, `domain`, `sha256`, ...).
    #[serde(rename = "type", default)]
    pub attr_type: String,
    /// Attribute category.
    #[serde(default)]
    pub category: String,
    /// Indicator value.
    #[serde(default)]
    pub value: String,
    /// Whether the attribute is auto-exported as an indicator.
    #[serde(default)]
    pub to_ids: bool,
    /// Unix timestamp string.
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Analyst comment.
    #[serde(default)]
    pub comment: Option<String>,
    /// Attached tags.
    #[serde(rename = "Tag", default)]
    pub tags: Vec<MispTag>,
    /// Unknown fields.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// A MISP tag attached to an attribute or event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MispTag {
    /// Tag name, e.g. `tlp:amber` or `misp-galaxy:mitre-attack-pattern="..."`
    #[serde(default)]
    pub name: String,
}

/// MISP threat-intel client. Auth is the raw `Authorization` header (MISP's
/// convention) plus `Accept: application/json`.
#[derive(Clone)]
pub struct MispClient {
    base: String,
    http: Client,
    auth: Auth,
}

impl MispClient {
    pub fn new(base_url: impl Into<String>, auth: Auth) -> Self {
        // TLS verifies by default; MISP_TLS_INSECURE=1 is an explicit
        // dev opt-in for self-signed compose MISP.
        let http = if crate::http::env("MISP_TLS_INSECURE").as_deref() == Some("1") {
            Client::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .unwrap_or_else(|_| Client::new())
        } else {
            Client::new()
        };
        Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            http,
            auth,
        }
    }

    /// MISP wants the raw key in `Authorization`, not a Bearer prefix.
    fn authed(&self, req: RequestBuilder) -> RequestBuilder {
        let req = req.header(header::ACCEPT, "application/json");
        match &self.auth {
            Auth::ApiKey(k) => req.header(header::AUTHORIZATION, k),
            other => other.apply(req),
        }
    }

    /// `POST /events/restSearch` — full-text value search, returns events.
    pub async fn search(&self, value: &str) -> Result<Vec<Event>> {
        let body = serde_json::json!({
            "returnFormat": "json",
            "value": value,
            "limit": 50,
        });
        let req = self
            .authed(self.http.post(join(&self.base, "/events/restSearch")))
            .json(&body);
        let resp = send_with_retry(req).await.context("misp search")?;
        let raw: Value = json_ok(resp).await?;
        // Shape: {"response": [{"Event": {...}}, ...]}
        let mut events = Vec::new();
        if let Some(rows) = raw.get("response").and_then(|r| r.as_array()) {
            for row in rows {
                let Some(ev) = row.get("Event") else { continue };
                match serde_json::from_value::<Event>(ev.clone()) {
                    Ok(e) => events.push(e),
                    Err(e) => tracing::warn!(error = %e, "misp event row skipped"),
                }
            }
        }
        Ok(events)
    }

    /// `POST /attributes/add/{event_id}`.
    pub async fn add_attribute(&self, event_id: &str, attr: &NewAttribute) -> Result<Value> {
        let event_id = segment(event_id, "event_id")?;
        let req = self
            .authed(
                self.http
                    .post(join(&self.base, &format!("/attributes/add/{event_id}"))),
            )
            .json(attr);
        json_ok(send_with_retry(req).await.context("misp add_attribute")?).await
    }

    /// `POST /attributes/restSearch` — attribute-level search; the shape is
    /// `{"response": {"Attribute": [...]}}`.
    pub async fn search_attributes(&self, value: &str, limit: u32) -> Result<Vec<MispAttribute>> {
        let body = serde_json::json!({
            "returnFormat": "json",
            "value": value,
            "limit": limit,
        });
        let req = self
            .authed(self.http.post(join(&self.base, "/attributes/restSearch")))
            .json(&body);
        let raw: Value = json_ok(
            send_with_retry(req)
                .await
                .context("misp search_attributes")?,
        )
        .await?;
        Ok(raw
            .pointer("/response/Attribute")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| match serde_json::from_value(r.clone()) {
                        Ok(attr) => Some(attr),
                        Err(e) => {
                            tracing::warn!(error = %e, "misp attribute row skipped");
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// `GET /events/view/{event_id}` — full event with attributes, objects,
    /// tags and galaxies.
    pub async fn event(&self, event_id: &str) -> Result<Value> {
        let event_id = segment(event_id, "event_id")?;
        let req = self.authed(
            self.http
                .get(join(&self.base, &format!("/events/view/{event_id}"))),
        );
        json_ok(send_with_retry(req).await.context("misp event")?).await
    }

    /// `POST /warninglists/checkValue` — returns the subset of `values` that
    /// match at least one warninglist (known-benign ranges, etc.).
    pub async fn check_warninglists(&self, values: &[String]) -> Result<Vec<String>> {
        let req = self
            .authed(self.http.post(join(&self.base, "/warninglists/checkValue")))
            .json(values);
        let raw: Value = json_ok(
            send_with_retry(req)
                .await
                .context("misp check_warninglists")?,
        )
        .await?;
        Ok(warninglist_matches(&raw))
    }

    /// `POST /sightings/add` — record sightings for attribute values.
    pub async fn add_sightings(&self, values: &[String], source: Option<&str>) -> Result<Value> {
        let mut body = serde_json::json!({ "values": values });
        if let Some(src) = source {
            body["source"] = Value::String(src.to_string());
        }
        let req = self
            .authed(self.http.post(join(&self.base, "/sightings/add")))
            .json(&body);
        json_ok(send_with_retry(req).await.context("misp add_sightings")?).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `attributes/restSearch` items carry numeric ids and nested `Tag` rows.
    #[test]
    fn misp_attribute_parses_rest_search_item() {
        let raw = serde_json::json!({
            "id": "12345",
            "event_id": "7",
            "type": "ip-dst",
            "category": "Network activity",
            "value": "203.0.113.9",
            "to_ids": true,
            "timestamp": "1712000000",
            "comment": "C2",
            "Tag": [{"name": "tlp:amber"}, {"name": "misp-galaxy:mitre-attack-pattern=\"C2\""}],
            "Event": {"info": "ignored by flatten"}
        });
        let a: MispAttribute = serde_json::from_value(raw).unwrap();
        assert_eq!(a.id, "12345");
        assert_eq!(a.event_id.as_deref(), Some("7"));
        assert!(a.to_ids);
        assert_eq!(a.tags.len(), 2);
        assert_eq!(a.tags[0].name, "tlp:amber");
        assert!(a.extra.contains_key("Event"));
    }

    /// Missing event_id stays `None` instead of failing the row.
    #[test]
    fn misp_attribute_tolerates_missing_event_id() {
        let a: MispAttribute = serde_json::from_value(serde_json::json!({
            "id": 1, "type": "domain", "value": "example.org"
        }))
        .unwrap();
        assert_eq!(a.event_id, None);
        assert!(!a.to_ids);
    }

    /// `checkValue` maps values to hit lists; empty lists are not matches.
    #[test]
    fn warninglist_matches_skips_empty_hit_lists() {
        let raw = serde_json::json!({
            "10.128.0.2": [{"id": "10", "name": "Wikimedia ranges"}],
            "8.8.8.8": []
        });
        assert_eq!(warninglist_matches(&raw), vec!["10.128.0.2".to_string()]);
        assert!(warninglist_matches(&serde_json::json!([])).is_empty());
    }
}
