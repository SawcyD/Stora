//! Optional cloud research for Advisor unknowns.
//!
//! This module receives a path only after an explicit UI action. It never
//! receives file contents, never changes a local policy verdict, and never
//! returns the API key it reads from Windows Credential Manager.

use serde::{Deserialize, Serialize};
use stora_core::{Result, StoraError};

const MODEL: &str = "gpt-4.1-mini-2025-04-14";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchAnswer {
    pub verdict: String,
    pub summary: String,
    pub reasons: Vec<String>,
    pub sources: Vec<ResearchSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchSource {
    pub title: String,
    pub url: String,
}

pub async fn research_path(path: &str) -> Result<ResearchAnswer> {
    let key = stora_winapi::read_advisor_api_key()?;
    let prompt = format!(
        "Research this Windows filesystem location: {path}\n\nReturn JSON only with verdict (reviewFirst, doNotRemove, or unknown), a short summary, 1-3 reasons, and sources (title and https URL). Use only primary vendor or official documentation as sources. Do not claim that a path is safe to delete. Do not request or infer file contents."
    );
    let schema = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["verdict", "summary", "reasons", "sources"],
        "properties": {
            "verdict": { "type": "string", "enum": ["reviewFirst", "doNotRemove", "unknown"] },
            "summary": { "type": "string" },
            "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 3 },
            "sources": { "type": "array", "items": { "type": "object", "additionalProperties": false, "required": ["title", "url"], "properties": { "title": { "type": "string" }, "url": { "type": "string" } } }, "maxItems": 3 }
        }
    });
    let body = serde_json::json!({
        "model": MODEL,
        "input": prompt,
        "text": { "format": { "type": "json_schema", "name": "stora_advisor", "strict": true, "schema": schema } },
        "max_output_tokens": 700
    });

    let response = reqwest::Client::new()
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|error| {
            StoraError::Internal(format!("Advisor research request failed: {error}"))
        })?;
    let status = response.status();
    let raw = response.text().await.map_err(|error| {
        StoraError::Internal(format!(
            "Advisor research response could not be read: {error}"
        ))
    })?;
    if !status.is_success() {
        return Err(StoraError::Internal(format!(
            "Advisor research failed ({status}): {raw}"
        )));
    }

    let response: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|_| StoraError::Internal("Advisor returned an unreadable response.".into()))?;
    let output = response_text(&response)
        .ok_or_else(|| StoraError::Internal("Advisor returned no research result.".into()))?;
    let mut answer: ResearchAnswer = serde_json::from_str(&output).map_err(|_| {
        StoraError::Internal("Advisor did not return the expected research format.".into())
    })?;

    // Citations are useful only when they are links a person can inspect.
    answer
        .sources
        .retain(|source| source.url.starts_with("https://"));
    Ok(answer)
}

/// Responses commonly includes a convenience `output_text`, but structured
/// output can instead arrive as an `output` message containing `content`
/// blocks. Support both shapes without exposing raw response data to the UI.
fn response_text(response: &serde_json::Value) -> Option<String> {
    response
        .get("output_text")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            response
                .get("output")
                .and_then(serde_json::Value::as_array)
                .and_then(|items| {
                    items.iter().find_map(|item| {
                        item.get("content")
                            .and_then(serde_json::Value::as_array)
                            .and_then(|content| {
                                content.iter().find_map(|block| {
                                    block
                                        .get("text")
                                        .and_then(serde_json::Value::as_str)
                                        .map(ToOwned::to_owned)
                                })
                            })
                    })
                })
        })
}

#[cfg(test)]
mod tests {
    use super::response_text;

    #[test]
    fn reads_convenience_output_text() {
        let value = serde_json::json!({ "output_text": "{\"verdict\":\"unknown\"}" });
        assert_eq!(
            response_text(&value).as_deref(),
            Some("{\"verdict\":\"unknown\"}")
        );
    }

    #[test]
    fn reads_message_content_text() {
        let value = serde_json::json!({
            "output": [{ "content": [{ "type": "output_text", "text": "{}" }] }]
        });
        assert_eq!(response_text(&value).as_deref(), Some("{}"));
    }
}
