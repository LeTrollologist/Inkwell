//! Provider-agnostic streaming chat.
//!
//! Three transports are supported: the OpenAI chat-completions dialect (used by
//! OpenAI, Groq, Mistral, DeepSeek, OpenRouter, xAI, Ollama, LM Studio and most
//! local servers), Anthropic's messages API and Google's Gemini stream API.
//! Everything is streamed from Rust so the API key never touches the webview.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::app::Emitter;
use crate::settings::{is_local_endpoint, ProviderConfig, DEFAULT_SYSTEM_PROMPT};

/// Roughly 30k tokens of document context.
pub const MAX_DOC_CHARS: usize = 120_000;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub id: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub document: Option<String>,
    #[serde(default)]
    pub selection: Option<String>,
    #[serde(default)]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub system_prompt: String,
}

fn default_max_tokens() -> u32 {
    1500
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeltaEvent {
    pub id: String,
    pub channel: &'static str,
    pub text: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoneEvent {
    pub id: String,
    pub reason: &'static str,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEvent {
    pub id: String,
    pub message: String,
}

#[derive(Debug)]
pub struct PreparedRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

#[derive(Debug, PartialEq)]
pub enum LineOutcome {
    Ignore,
    Deltas(Vec<(&'static str, String)>),
    Error(String),
    Done,
}

fn truncate_chars(source: &str, max: usize) -> String {
    if source.chars().count() <= max {
        return source.to_string();
    }
    let cut: String = source.chars().take(max).collect();
    format!("{cut}\n\n[... document truncated at {max} characters ...]")
}

pub fn compose_system(req: &ChatRequest) -> String {
    let mut system = if req.system_prompt.trim().is_empty() {
        DEFAULT_SYSTEM_PROMPT.to_string()
    } else {
        req.system_prompt.trim().to_string()
    };
    if let Some(doc) = req.document.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        system.push_str("\n\n<document>\n");
        system.push_str(&truncate_chars(doc, MAX_DOC_CHARS));
        system.push_str("\n</document>");
    }
    if let Some(selection) = req.selection.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        system.push_str("\n\nThe user highlighted this passage:\n<selection>\n");
        system.push_str(&truncate_chars(selection, 8_000));
        system.push_str("\n</selection>");
    }
    system
}

fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        let ch = *byte as char;
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~') {
            out.push(ch);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Appends `/v1` for hosts given without a path (e.g. https://api.openai.com).
fn openai_base(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    let after_scheme = base.splitn(2, "://").nth(1).unwrap_or(base);
    if after_scheme.contains('/') {
        base.to_string()
    } else {
        format!("{base}/v1")
    }
}

fn anthropic_url(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    let base = if base.is_empty() {
        "https://api.anthropic.com"
    } else {
        base
    };
    if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
}

fn gemini_url(base: &str, model: &str, key: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    let base = if base.is_empty() {
        "https://generativelanguage.googleapis.com"
    } else {
        base
    };
    let base = base.trim_end_matches("/v1beta");
    let model = model.trim_start_matches("models/");
    let mut url = format!("{base}/v1beta/models/{model}:streamGenerateContent?alt=sse");
    if !key.is_empty() {
        url.push_str("&key=");
        url.push_str(&url_encode(key));
    }
    url
}

fn merge_alternating(messages: Vec<ChatMessage>, assistant_role: &str) -> Vec<ChatMessage> {
    let mut out: Vec<ChatMessage> = Vec::new();
    for message in messages {
        let role = if message.role == "assistant" { assistant_role } else { "user" };
        if message.content.trim().is_empty() {
            continue;
        }
        if let Some(last) = out.last_mut() {
            if last.role == role {
                last.content.push_str("\n\n");
                last.content.push_str(&message.content);
                continue;
            }
        }
        out.push(ChatMessage {
            role: role.to_string(),
            content: message.content,
        });
    }
    out
}

fn text_of(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let joined: String = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect();
            if joined.is_empty() {
                None
            } else {
                Some(joined)
            }
        }
        _ => None,
    }
}

pub fn build_request(
    provider: &ProviderConfig,
    req: &ChatRequest,
) -> Result<PreparedRequest, String> {
    let key = provider.api_key.trim();
    let model = provider.model.trim();
    if model.is_empty() {
        return Err(format!(
            "No model set for {}. Open Settings (Ctrl+,) and choose one.",
            provider.label
        ));
    }
    if key.is_empty() && !is_local_endpoint(&provider.base_url) {
        return Err(format!(
            "{} needs an API key. Open Settings (Ctrl+,) to add one.",
            provider.label
        ));
    }

    let system = compose_system(req);
    let temperature = req.temperature.clamp(0.0, 2.0);
    let max_tokens = req.max_tokens.clamp(64, 32_000);

    match provider.kind.as_str() {
        "anthropic" => {
            let messages = merge_alternating(req.messages.clone(), "assistant");
            let mut body = json!({
                "model": model,
                "max_tokens": max_tokens,
                "temperature": temperature,
                "stream": true,
                "messages": messages,
            });
            if !system.trim().is_empty() {
                body["system"] = json!(system);
            }
            Ok(PreparedRequest {
                url: anthropic_url(&provider.base_url),
                headers: vec![
                    ("content-type".into(), "application/json".into()),
                    ("x-api-key".into(), key.to_string()),
                    ("anthropic-version".into(), "2023-06-01".into()),
                ],
                body,
            })
        }
        "gemini" => {
            let contents: Vec<Value> = merge_alternating(req.messages.clone(), "model")
                .into_iter()
                .map(|message| {
                    json!({
                        "role": message.role,
                        "parts": [{ "text": message.content }],
                    })
                })
                .collect();
            let mut body = json!({
                "contents": contents,
                "generationConfig": {
                    "temperature": temperature,
                    "maxOutputTokens": max_tokens,
                },
            });
            if !system.trim().is_empty() {
                body["systemInstruction"] = json!({ "parts": [{ "text": system }] });
            }
            Ok(PreparedRequest {
                url: gemini_url(&provider.base_url, model, key),
                headers: vec![("content-type".into(), "application/json".into())],
                body,
            })
        }
        _ => {
            let mut messages: Vec<Value> = Vec::new();
            if !system.trim().is_empty() {
                messages.push(json!({ "role": "system", "content": system }));
            }
            for message in &req.messages {
                let role = if message.role == "assistant" {
                    "assistant"
                } else {
                    "user"
                };
                if message.content.trim().is_empty() {
                    continue;
                }
                messages.push(json!({ "role": role, "content": message.content }));
            }
            let body = json!({
                "model": model,
                "stream": true,
                "temperature": temperature,
                "max_tokens": max_tokens,
                "messages": messages,
            });
            let mut headers = vec![("content-type".into(), "application/json".into())];
            if !key.is_empty() {
                headers.push(("authorization".into(), format!("Bearer {key}")));
            }
            if provider.base_url.contains("openrouter.ai") {
                headers.push(("x-title".into(), "Inkwell".into()));
            }
            Ok(PreparedRequest {
                url: format!("{}/chat/completions", openai_base(&provider.base_url)),
                headers,
                body,
            })
        }
    }
}
/// Parses one SSE line into incremental deltas.
pub fn parse_sse_line(kind: &str, line: &str) -> LineOutcome {
    let line = line.trim();
    if line.is_empty()
        || line.starts_with(':')
        || line.starts_with("event:")
        || line.starts_with("id:")
    {
        return LineOutcome::Ignore;
    }
    let Some(data) = line.strip_prefix("data:") else {
        return LineOutcome::Ignore;
    };
    let data = data.trim();
    if data.is_empty() {
        return LineOutcome::Ignore;
    }
    if data == "[DONE]" {
        return LineOutcome::Done;
    }
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        return LineOutcome::Ignore;
    };
    extract_deltas(kind, &value)
}

fn provider_error(value: &Value) -> Option<String> {
    let error = value.get("error")?;
    if error.is_null() {
        return None;
    }
    Some(
        error
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| error.as_str())
            .unwrap_or("the provider returned an error")
            .to_string(),
    )
}

pub fn extract_deltas(kind: &str, value: &Value) -> LineOutcome {
    if let Some(message) = provider_error(value) {
        return LineOutcome::Error(message);
    }

    let mut deltas: Vec<(&'static str, String)> = Vec::new();
    match kind {
        "anthropic" => {
            if value.get("type").and_then(Value::as_str) == Some("content_block_delta") {
                let delta = &value["delta"];
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        if let Some(text) = delta.get("text").and_then(Value::as_str) {
                            deltas.push(("content", text.to_string()));
                        }
                    }
                    Some("thinking_delta") => {
                        if let Some(text) = delta.get("thinking").and_then(Value::as_str) {
                            deltas.push(("reasoning", text.to_string()));
                        }
                    }
                    _ => {}
                }
            }
        }
        "gemini" => {
            let parts = value["candidates"][0]["content"]["parts"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for part in parts {
                if let Some(text) = part.get("text").and_then(Value::as_str) {
                    if part.get("thought").and_then(Value::as_bool).unwrap_or(false) {
                        deltas.push(("reasoning", text.to_string()));
                    } else {
                        deltas.push(("content", text.to_string()));
                    }
                }
            }
        }
        _ => {
            let delta = &value["choices"][0]["delta"];
            if let Some(reasoning) =
                text_of(&delta["reasoning_content"]).or_else(|| text_of(&delta["reasoning"]))
            {
                deltas.push(("reasoning", reasoning));
            }
            if let Some(content) = text_of(&delta["content"]) {
                deltas.push(("content", content));
            }
            if deltas.is_empty() {
                if let Some(text) = text_of(&value["choices"][0]["text"]) {
                    deltas.push(("content", text));
                }
            }
        }
    }

    if deltas.is_empty() {
        LineOutcome::Ignore
    } else {
        LineOutcome::Deltas(deltas)
    }
}

fn friendly_http_error(status: reqwest::StatusCode, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| provider_error(&value))
        .unwrap_or_else(|| {
            let trimmed = body.trim();
            if trimmed.is_empty() {
                status
                    .canonical_reason()
                    .unwrap_or("request failed")
                    .to_string()
            } else {
                trimmed.chars().take(600).collect()
            }
        });
    match status.as_u16() {
        401 | 403 => format!("Authentication failed ({status}). Check the API key. {detail}"),
        404 => format!("Endpoint not found ({status}). Check the base URL and model. {detail}"),
        429 => format!("Rate limited or out of quota ({status}). {detail}"),
        _ => format!("Provider error ({status}): {detail}"),
    }
}

/// Runs a chat completion, emitting `llm:delta` / `llm:done` / `llm:error`.
pub async fn stream_chat(
    emitter: Emitter,
    client: reqwest::Client,
    provider: ProviderConfig,
    req: ChatRequest,
    cancel: Arc<AtomicBool>,
) {
    let id = req.id.clone();
    let emit_done = |reason: &'static str| {
        emitter.event(
            "llm:done",
            DoneEvent {
                id: id.clone(),
                reason,
            },
        );
    };
    let emit_error = |message: String| {
        emitter.event(
            "llm:error",
            ErrorEvent {
                id: id.clone(),
                message,
            },
        );
    };

    let prepared = match build_request(&provider, &req) {
        Ok(prepared) => prepared,
        Err(message) => {
            emit_error(message);
            return;
        }
    };

    let mut builder = client
        .post(&prepared.url)
        .header("accept", "text/event-stream")
        .json(&prepared.body);
    for (name, value) in &prepared.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }

    let response = match builder.send().await {
        Ok(response) => response,
        Err(err) => {
            emit_error(format!(
                "Could not reach {}: {err}",
                provider.base_url.trim_end_matches('/')
            ));
            return;
        }
    };

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        emit_error(friendly_http_error(status, &body));
        return;
    }

    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::with_capacity(8192);
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            emit_done("cancelled");
            return;
        }
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(err) => {
                emit_error(format!("The stream ended unexpectedly: {err}"));
                return;
            }
        };
        buffer.extend_from_slice(&chunk);

        while let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = buffer.drain(..=position).collect();
            let line = String::from_utf8_lossy(&line[..line.len().saturating_sub(1)]);
            match parse_sse_line(&provider.kind, line.trim_end_matches('\r')) {
                LineOutcome::Deltas(deltas) => {
                    for (channel, text) in deltas {
                        emitter.event(
                            "llm:delta",
                            DeltaEvent {
                                id: id.clone(),
                                channel,
                                text,
                            },
                        );
                    }
                }
                LineOutcome::Error(message) => {
                    emit_error(message);
                    return;
                }
                LineOutcome::Done => {
                    emit_done("stop");
                    return;
                }
                LineOutcome::Ignore => {}
            }
        }
    }

    if cancel.load(Ordering::Relaxed) {
        emit_done("cancelled");
    } else {
        emit_done("stop");
    }
}

/// Fetches the model list for the "refresh models" button.
pub async fn fetch_models(
    client: &reqwest::Client,
    provider: &ProviderConfig,
) -> Result<Vec<String>, String> {
    let base = provider.base_url.trim().trim_end_matches('/');
    let key = provider.api_key.trim();
    if key.is_empty() && !is_local_endpoint(base) {
        return Err(format!(
            "{} needs an API key before its models can be listed.",
            provider.label
        ));
    }

    let mut request = match provider.kind.as_str() {
        "anthropic" => {
            let base = if base.is_empty() {
                "https://api.anthropic.com"
            } else {
                base
            };
            let url = if base.ends_with("/v1") {
                format!("{base}/models")
            } else {
                format!("{base}/v1/models")
            };
            client
                .get(url)
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
        }
        "gemini" => {
            let base = if base.is_empty() {
                "https://generativelanguage.googleapis.com"
            } else {
                base.trim_end_matches("/v1beta")
            };
            client.get(format!("{base}/v1beta/models?key={}", url_encode(key)))
        }
        _ => {
            let mut request = client.get(format!("{}/models", openai_base(base)));
            if !key.is_empty() {
                request = request.bearer_auth(key);
            }
            request
        }
    };
    request = request.header("accept", "application/json");

    let response = request
        .send()
        .await
        .map_err(|err| format!("Could not reach {}: {err}", provider.base_url))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(friendly_http_error(status, &body));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|err| format!("Unexpected response from {}: {err}", provider.label))?;

    let mut models: Vec<String> = Vec::new();
    if let Some(items) = value["data"].as_array() {
        for item in items {
            if let Some(id) = item["id"].as_str() {
                models.push(id.to_string());
            }
        }
    }
    if let Some(items) = value["models"].as_array() {
        for item in items {
            if let Some(name) = item["name"].as_str() {
                models.push(name.trim_start_matches("models/").to_string());
            } else if let Some(id) = item["id"].as_str() {
                models.push(id.to_string());
            }
        }
    }
    models.sort();
    models.dedup();
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(kind: &str) -> ProviderConfig {
        ProviderConfig {
            id: "test".into(),
            label: "Test".into(),
            kind: kind.into(),
            base_url: "https://api.example.com".into(),
            api_key: "sk-test".into(),
            model: "model-1".into(),
        }
    }

    fn request() -> ChatRequest {
        ChatRequest {
            id: "req-1".into(),
            messages: vec![
                ChatMessage {
                    role: "user".into(),
                    content: "first".into(),
                },
                ChatMessage {
                    role: "user".into(),
                    content: "second".into(),
                },
            ],
            document: Some("# Doc\n\nBody".into()),
            selection: Some("Body".into()),
            temperature: 0.2,
            max_tokens: 400,
            system_prompt: "Be terse.".into(),
        }
    }

    #[test]
    fn openai_request_uses_chat_completions_and_bearer_auth() {
        let prepared = build_request(&provider("openai"), &request()).unwrap();
        assert_eq!(prepared.url, "https://api.example.com/v1/chat/completions");
        assert!(prepared
            .headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer sk-test"));
        assert_eq!(prepared.body["stream"], true);
        assert_eq!(prepared.body["model"], "model-1");
        let messages = prepared.body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert!(messages[0]["content"].as_str().unwrap().contains("<document>"));
        assert_eq!(messages[1]["content"], "first");
    }

    #[test]
    fn openai_base_gets_v1_only_when_missing_a_path() {
        let mut cfg = provider("openai");
        cfg.base_url = "https://api.openai.com".into();
        assert_eq!(
            build_request(&cfg, &request()).unwrap().url,
            "https://api.openai.com/v1/chat/completions"
        );
        cfg.base_url = "http://localhost:11434/v1/".into();
        cfg.api_key.clear();
        assert_eq!(
            build_request(&cfg, &request()).unwrap().url,
            "http://localhost:11434/v1/chat/completions"
        );
    }

    #[test]
    fn anthropic_request_merges_turns_and_sends_system() {
        let prepared = build_request(&provider("anthropic"), &request()).unwrap();
        assert_eq!(prepared.url, "https://api.example.com/v1/messages");
        assert!(prepared
            .headers
            .iter()
            .any(|(k, v)| k == "x-api-key" && v == "sk-test"));
        let messages = prepared.body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(messages[0]["content"].as_str().unwrap().contains("first"));
        assert!(prepared.body["system"].as_str().unwrap().contains("<selection>"));
    }

    #[test]
    fn gemini_request_embeds_model_and_key() {
        let mut cfg = provider("gemini");
        cfg.base_url = "https://generativelanguage.googleapis.com".into();
        let prepared = build_request(&cfg, &request()).unwrap();
        assert!(prepared
            .url
            .contains("/v1beta/models/model-1:streamGenerateContent"));
        assert!(prepared.url.contains("key=sk-test"));
        assert_eq!(prepared.body["contents"][0]["role"], "user");
        assert_eq!(
            prepared.body["generationConfig"]["maxOutputTokens"],
            serde_json::json!(400)
        );
    }

    #[test]
    fn remote_provider_requires_a_key() {
        let mut cfg = provider("openai");
        cfg.api_key.clear();
        let err = build_request(&cfg, &request()).unwrap_err();
        assert!(err.contains("needs an API key"));
    }

    #[test]
    fn sse_lines_are_parsed_per_provider() {
        assert_eq!(parse_sse_line("openai", ": keep-alive"), LineOutcome::Ignore);
        assert_eq!(parse_sse_line("openai", "data: [DONE]"), LineOutcome::Done);
        assert_eq!(
            parse_sse_line("openai", r#"data: {"choices":[{"delta":{"content":"Hi"}}]}"#),
            LineOutcome::Deltas(vec![("content", "Hi".to_string())])
        );
        assert_eq!(
            parse_sse_line(
                "openai",
                r#"data: {"choices":[{"delta":{"reasoning_content":"hmm"}}]}"#
            ),
            LineOutcome::Deltas(vec![("reasoning", "hmm".to_string())])
        );
        assert_eq!(
            parse_sse_line(
                "anthropic",
                r#"data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"Yo"}}"#
            ),
            LineOutcome::Deltas(vec![("content", "Yo".to_string())])
        );
        assert_eq!(
            parse_sse_line(
                "anthropic",
                r#"data: {"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"why"}}"#
            ),
            LineOutcome::Deltas(vec![("reasoning", "why".to_string())])
        );
        assert_eq!(
            parse_sse_line(
                "gemini",
                r#"data: {"candidates":[{"content":{"parts":[{"text":"think","thought":true},{"text":"answer"}]}}]}"#
            ),
            LineOutcome::Deltas(vec![
                ("reasoning", "think".to_string()),
                ("content", "answer".to_string())
            ])
        );
        assert_eq!(
            parse_sse_line("openai", r#"data: {"error":{"message":"bad key"}}"#),
            LineOutcome::Error("bad key".to_string())
        );
        assert_eq!(parse_sse_line("openai", "data: not-json"), LineOutcome::Ignore);
    }

    #[test]
    fn document_context_is_truncated() {
        let mut req = request();
        req.document = Some("x".repeat(MAX_DOC_CHARS + 50));
        let system = compose_system(&req);
        assert!(system.contains("document truncated"));
        assert!(system.len() < MAX_DOC_CHARS + 2000);
    }

    #[test]
    fn errors_are_humanised() {
        let message = friendly_http_error(
            reqwest::StatusCode::UNAUTHORIZED,
            r#"{"error":{"message":"Invalid API key"}}"#,
        );
        assert!(message.contains("Authentication failed"));
        assert!(message.contains("Invalid API key"));
    }
}

