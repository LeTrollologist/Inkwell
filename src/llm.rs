//! Provider-agnostic streaming chat.
//!
//! Three transports are supported: the OpenAI chat-completions dialect (used by
//! OpenAI, Groq, Mistral, DeepSeek, OpenRouter, xAI, Ollama, LM Studio and most
//! local servers), Anthropic's messages API and Google's Gemini stream API.
//! Everything is streamed from Rust so the API key never touches the webview.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::app::Emitter;
use crate::search::SearchHit;
use crate::settings::{is_local_endpoint, ProviderConfig, DEFAULT_SYSTEM_PROMPT};

/// Roughly 30k tokens of document context.
pub const MAX_DOC_CHARS: usize = 120_000;
/// Largest unterminated SSE line buffered from a provider.
const MAX_SSE_BUFFER: usize = 2 * 1024 * 1024;

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
    /// `ask`, `search` or `edit`.
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub thinking: bool,
    #[serde(default)]
    pub search_results: Vec<SearchHit>,
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

/// Destination for streamed chat events. `Emitter` forwards to the webview;
/// tests use an in-memory recorder so cancellation and error paths can be
/// exercised without an event loop.
pub trait StreamSink: Send + Sync + 'static {
    fn delta(&self, id: &str, channel: &'static str, text: &str);
    fn done(&self, id: &str, reason: &'static str);
    fn error(&self, id: &str, message: &str);
}

impl StreamSink for Emitter {
    fn delta(&self, id: &str, channel: &'static str, text: &str) {
        self.event(
            "llm:delta",
            DeltaEvent {
                id: id.to_string(),
                channel,
                text: text.to_string(),
            },
        );
    }

    fn done(&self, id: &str, reason: &'static str) {
        self.event(
            "llm:done",
            DoneEvent {
                id: id.to_string(),
                reason,
            },
        );
    }

    fn error(&self, id: &str, message: &str) {
        self.event(
            "llm:error",
            ErrorEvent {
                id: id.to_string(),
                message: message.to_string(),
            },
        );
    }
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
    /// Deltas followed by the provider's terminal marker in the same chunk.
    DeltasDone(Vec<(&'static str, String)>),
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
    if !req.search_results.is_empty() {
        system.push_str("\n\n<search_results>\n");
        for (index, hit) in req.search_results.iter().enumerate() {
            let heading = if hit.heading.trim().is_empty() {
                "Document"
            } else {
                hit.heading.as_str()
            };
            system.push_str(&format!(
                "[{}] {} (line {})\n{}\n\n",
                index + 1,
                heading,
                hit.line,
                hit.snippet
            ));
        }
        system.push_str(
            "</search_results>\nThese passages were retrieved by searching the document locally. \
             Base the answer on them when relevant, cite the heading in square brackets, and say \
             when the retrieved passages do not contain the answer.",
        );
    } else if req.mode == "search" && req.document.is_none() {
        system.push_str(
            "\n\nNo passages matched the local document search. Tell the user that the document \
             does not appear to contain the answer; do not invent a source.",
        );
    }
    if req.mode == "edit" {
        system.push_str(
            "\n\nYou are editing the full document above. Apply the user's requested change and \
             return the COMPLETE revised Markdown document in exactly one fenced block delimited \
             by four backticks, like ````markdown ... ````. Preserve front matter and everything \
             the user did not ask to change. Do not add commentary outside the fenced block.",
        );
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

fn is_reasoning_model(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
        || model.starts_with("gpt-5")
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
            if req.thinking {
                let budget = (max_tokens / 2).clamp(1024, 4096);
                body["max_tokens"] = json!(max_tokens.max(budget + 512));
                // Anthropic requires temperature 1 when extended thinking is on.
                body["temperature"] = json!(1.0);
                body["thinking"] = json!({ "type": "enabled", "budget_tokens": budget });
            }
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
            let mut generation = json!({
                "temperature": temperature,
                "maxOutputTokens": max_tokens,
            });
            if req.thinking {
                generation["thinkingConfig"] = json!({
                    "includeThoughts": true,
                    "thinkingBudget": 1024
                });
            }
            let mut body = json!({
                "contents": contents,
                "generationConfig": generation,
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
            let reasoning_model = is_reasoning_model(model);
            let mut body = json!({
                "model": model,
                "stream": true,
                "messages": messages,
            });
            if reasoning_model {
                body["temperature"] = json!(1.0);
                body["max_completion_tokens"] = json!(max_tokens);
            } else {
                body["temperature"] = json!(temperature);
                body["max_tokens"] = json!(max_tokens);
            }
            if req.thinking && (reasoning_model || provider.base_url.contains("openrouter.ai")) {
                body["reasoning_effort"] = json!("medium");
            }
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
    let mut terminal = false;

    match kind {
        "anthropic" => {
            let event_type = value.get("type").and_then(Value::as_str).unwrap_or_default();
            match event_type {
                "content_block_delta" => {
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
                "message_stop" => terminal = true,
                // Some compatible servers omit `message_stop` but do set a
                // stop_reason on the final message_delta.
                "message_delta" => {
                    terminal = !value["delta"]["stop_reason"].is_null();
                }
                _ => {}
            }
        }
        "gemini" => {
            let candidate = &value["candidates"][0];
            let parts = candidate["content"]["parts"]
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
            terminal = !candidate["finishReason"].is_null();
        }
        _ => {
            let choice = &value["choices"][0];
            let delta = &choice["delta"];
            if let Some(reasoning) =
                text_of(&delta["reasoning_content"]).or_else(|| text_of(&delta["reasoning"]))
            {
                deltas.push(("reasoning", reasoning));
            }
            if let Some(content) = text_of(&delta["content"]) {
                deltas.push(("content", content));
            }
            if deltas.is_empty() {
                if let Some(text) = text_of(&choice["text"]) {
                    deltas.push(("content", text));
                }
            }
            terminal = !choice["finish_reason"].is_null()
                && choice["finish_reason"]
                    .as_str()
                    .map(|reason| !reason.is_empty())
                    .unwrap_or(true);
        }
    }

    match (deltas.is_empty(), terminal) {
        (true, true) => LineOutcome::Done,
        (true, false) => LineOutcome::Ignore,
        (false, true) => LineOutcome::DeltasDone(deltas),
        (false, false) => LineOutcome::Deltas(deltas),
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
        408 => format!("The provider timed out ({status}). Try again. {detail}"),
        429 => format!("Rate limited or out of quota ({status}). {detail}"),
        500..=599 => format!(
            "The provider had a server error ({status}). Try again shortly. {detail}"
        ),
        _ => format!("Provider error ({status}): {detail}"),
    }
}

/// Runs a chat completion, emitting `llm:delta` / `llm:done` / `llm:error`.
///
/// Cancellation is polled while waiting for headers and while waiting for
/// body chunks, so a stalled provider does not hold the UI hostage.
pub async fn stream_chat<S: StreamSink>(
    sink: S,
    client: reqwest::Client,
    provider: ProviderConfig,
    req: ChatRequest,
    cancel: Arc<AtomicBool>,
) {
    let id = req.id.clone();

    let prepared = match build_request(&provider, &req) {
        Ok(prepared) => prepared,
        Err(message) => {
            sink.error(&id, &message);
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

    let response = tokio::select! {
        biased;
        _ = wait_for_cancel(&cancel) => {
            sink.done(&id, "cancelled");
            return;
        }
        result = builder.send() => result,
    };

    let response = match response {
        Ok(response) => response,
        Err(err) => {
            sink.error(
                &id,
                &format!(
                    "Could not reach {}: {err}",
                    provider.base_url.trim_end_matches('/')
                ),
            );
            return;
        }
    };

    let status = response.status();
    if !status.is_success() {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = tokio::select! {
            biased;
            _ = wait_for_cancel(&cancel) => {
                sink.done(&id, "cancelled");
                return;
            }
            result = response.text() => result.unwrap_or_default(),
        };
        let mut message = friendly_http_error(status, &body);
        if let Some(retry) = retry_after {
            message.push_str(&format!(" The provider suggests retrying after {retry}."));
        }
        sink.error(&id, &message);
        return;
    }

    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::with_capacity(8192);
    let mut saw_content = false;

    loop {
        let next = tokio::select! {
            biased;
            _ = wait_for_cancel(&cancel) => {
                sink.done(&id, "cancelled");
                return;
            }
            item = stream.next() => item,
        };
        let Some(chunk) = next else {
            break;
        };
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(err) => {
                sink.error(&id, &format!("The stream ended unexpectedly: {err}"));
                return;
            }
        };
        buffer.extend_from_slice(&chunk);
        if buffer.len() > MAX_SSE_BUFFER {
            sink.error(&id, "The provider sent an unexpectedly large stream line.");
            return;
        }

        while let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = buffer.drain(..=position).collect();
            let line = String::from_utf8_lossy(&line[..line.len().saturating_sub(1)]);
            match parse_sse_line(&provider.kind, line.trim_end_matches('\r')) {
                LineOutcome::Deltas(deltas) => {
                    saw_content = true;
                    for (channel, text) in deltas {
                        sink.delta(&id, channel, &text);
                    }
                }
                LineOutcome::DeltasDone(deltas) => {
                    for (channel, text) in deltas {
                        sink.delta(&id, channel, &text);
                    }
                    sink.done(&id, "stop");
                    return;
                }
                LineOutcome::Error(message) => {
                    sink.error(&id, &message);
                    return;
                }
                LineOutcome::Done => {
                    sink.done(&id, "stop");
                    return;
                }
                LineOutcome::Ignore => {}
            }
        }
    }

    if cancel.load(Ordering::Relaxed) {
        sink.done(&id, "cancelled");
    } else if !saw_content {
        sink.error(
            &id,
            "The provider closed the stream without sending a response.",
        );
    } else if matches!(provider.kind.as_str(), "anthropic" | "gemini") {
        sink.error(
            &id,
            "The stream ended before the provider finished. Check the connection and try again.",
        );
    } else {
        // Many OpenAI-compatible local servers close the body without [DONE].
        sink.done(&id, "stop");
    }
}

async fn wait_for_cancel(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(50)).await;
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
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

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
            mode: "ask".into(),
            thinking: false,
            search_results: Vec::new(),
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
    fn reasoning_models_use_reasoning_controls() {
        let mut cfg = provider("openai");
        cfg.model = "o3-mini".into();
        let mut req = request();
        req.thinking = true;
        let prepared = build_request(&cfg, &req).unwrap();
        assert_eq!(prepared.body["max_completion_tokens"], 400);
        assert!(prepared.body.get("max_tokens").is_none());
        assert_eq!(prepared.body["reasoning_effort"], "medium");
        assert_eq!(prepared.body["temperature"], 1.0);
    }

    #[test]
    fn anthropic_thinking_sets_a_budget() {
        let mut req = request();
        req.thinking = true;
        let prepared = build_request(&provider("anthropic"), &req).unwrap();
        assert_eq!(prepared.body["thinking"]["type"], "enabled");
        assert_eq!(prepared.body["thinking"]["budget_tokens"], 1024);
        assert_eq!(prepared.body["temperature"], 1.0);
        assert!(prepared.body["max_tokens"].as_u64().unwrap() > 1024);
    }

    #[test]
    fn gemini_thinking_requests_thoughts() {
        let mut req = request();
        req.thinking = true;
        let prepared = build_request(&provider("gemini"), &req).unwrap();
        assert_eq!(
            prepared.body["generationConfig"]["thinkingConfig"]["includeThoughts"],
            true
        );
    }

    #[test]
    fn search_results_and_edit_mode_reach_the_system_prompt() {
        let mut req = request();
        req.mode = "search".into();
        req.search_results = vec![SearchHit {
            heading: "Installation > Windows".into(),
            line: 12,
            snippet: "Run the installer.".into(),
            score: 1.0,
        }];
        let prompt = compose_system(&req);
        assert!(prompt.contains("<search_results>"));
        assert!(prompt.contains("Installation > Windows"));
        assert!(prompt.contains("Run the installer."));

        req.mode = "edit".into();
        let prompt = compose_system(&req);
        assert!(prompt.contains("fenced block"));
        assert!(prompt.contains("COMPLETE revised Markdown"));
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

    #[derive(Debug, Clone, PartialEq)]
    enum TestEvent {
        Delta {
            id: String,
            channel: &'static str,
            text: String,
        },
        Done {
            id: String,
            reason: &'static str,
        },
        Error {
            id: String,
            message: String,
        },
    }

    #[derive(Clone)]
    struct TestSink(Arc<Mutex<Vec<TestEvent>>>);

    impl TestSink {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(Vec::new())))
        }

        fn events(&self) -> Vec<TestEvent> {
            self.0.lock().unwrap().clone()
        }
    }

    impl StreamSink for TestSink {
        fn delta(&self, id: &str, channel: &'static str, text: &str) {
            self.0.lock().unwrap().push(TestEvent::Delta {
                id: id.to_string(),
                channel,
                text: text.to_string(),
            });
        }

        fn done(&self, id: &str, reason: &'static str) {
            self.0.lock().unwrap().push(TestEvent::Done {
                id: id.to_string(),
                reason,
            });
        }

        fn error(&self, id: &str, message: &str) {
            self.0.lock().unwrap().push(TestEvent::Error {
                id: id.to_string(),
                message: message.to_string(),
            });
        }
    }

    struct MockResponse {
        status: &'static str,
        headers: Vec<(&'static str, String)>,
        body: Vec<u8>,
        chunk_delay: Option<Duration>,
        content_length: Option<usize>,
    }

    async fn spawn_server(response: MockResponse) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 8192];
            let _ = socket.read(&mut request).await;

            let mut head = format!("HTTP/1.1 {}\r\nConnection: close\r\n", response.status);
            for (name, value) in &response.headers {
                head.push_str(&format!("{name}: {value}\r\n"));
            }
            if let Some(len) = response.content_length {
                head.push_str(&format!("Content-Length: {len}\r\n"));
            }
            head.push_str("\r\n");
            if socket.write_all(head.as_bytes()).await.is_err() {
                return;
            }
            for chunk in response.body.chunks(64) {
                if socket.write_all(chunk).await.is_err() {
                    return;
                }
                let _ = socket.flush().await;
                if let Some(delay) = response.chunk_delay {
                    tokio::time::sleep(delay).await;
                }
            }
            let _ = socket.shutdown().await;
        });
        (format!("http://{addr}"), handle)
    }

    fn provider_at(kind: &str, base: &str) -> ProviderConfig {
        let mut cfg = provider(kind);
        cfg.base_url = base.to_string();
        cfg
    }

    fn request_with_id(id: &str) -> ChatRequest {
        let mut req = request();
        req.id = id.to_string();
        req
    }

    fn sse_body(lines: &[&str]) -> Vec<u8> {
        lines
            .iter()
            .map(|line| format!("data: {line}\n\n"))
            .collect::<String>()
            .into_bytes()
    }

    #[test]
    fn terminal_markers_finish_the_stream() {
        assert_eq!(
            parse_sse_line(
                "openai",
                r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#
            ),
            LineOutcome::Done
        );
        assert_eq!(
            parse_sse_line(
                "openai",
                r#"data: {"choices":[{"delta":{"content":"x"},"finish_reason":"stop"}]}"#
            ),
            LineOutcome::DeltasDone(vec![("content", "x".to_string())])
        );
        assert_eq!(
            parse_sse_line("anthropic", r#"data: {"type":"message_stop"}"#),
            LineOutcome::Done
        );
        assert_eq!(
            parse_sse_line(
                "gemini",
                r#"data: {"candidates":[{"content":{"parts":[{"text":"hi"}]},"finishReason":"STOP"}]}"#
            ),
            LineOutcome::DeltasDone(vec![("content", "hi".to_string())])
        );
        assert_eq!(
            parse_sse_line(
                "gemini",
                r#"data: {"candidates":[{"finishReason":"MAX_TOKENS"}]}"#
            ),
            LineOutcome::Done
        );
    }


    #[tokio::test]
    async fn streams_deltas_and_done_over_http() {
        let body = sse_body(&[
            r#"{"choices":[{"delta":{"content":"Hel"}}]}"#,
            r#"{"choices":[{"delta":{"content":"lo"}}]}"#,
            "[DONE]",
        ]);
        let (url, server) = spawn_server(MockResponse {
            status: "200 OK",
            headers: vec![("Content-Type", "text/event-stream".into())],
            body,
            chunk_delay: None,
            content_length: None,
        })
        .await;

        let sink = TestSink::new();
        let cancel = Arc::new(AtomicBool::new(false));
        tokio::time::timeout(
            Duration::from_secs(5),
            stream_chat(
                sink.clone(),
                reqwest::Client::new(),
                provider_at("openai", &url),
                request_with_id("req-a"),
                cancel,
            ),
        )
        .await
        .expect("stream timed out");

        let events = sink.events();
        let text: String = events
            .iter()
            .filter_map(|event| match event {
                TestEvent::Delta { id, text, .. } if id.as_str() == "req-a" => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(text, "Hello");
        assert!(events.iter().any(
            |event| matches!(event, TestEvent::Done { id, reason } if id.as_str() == "req-a" && *reason == "stop")
        ));
        server.abort();
    }

    #[tokio::test]
    async fn cancellation_breaks_a_stalled_stream() {
        let body = sse_body(&[r#"{"choices":[{"delta":{"content":"one"}}]}"#]);
        let (url, server) = spawn_server(MockResponse {
            status: "200 OK",
            headers: vec![("Content-Type", "text/event-stream".into())],
            body,
            chunk_delay: Some(Duration::from_secs(10)),
            content_length: None,
        })
        .await;

        let sink = TestSink::new();
        let cancel = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn(stream_chat(
            sink.clone(),
            reqwest::Client::new(),
            provider_at("openai", &url),
            request_with_id("cancel-1"),
            cancel.clone(),
        ));

        tokio::time::sleep(Duration::from_millis(300)).await;
        cancel.store(true, Ordering::Relaxed);

        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("cancellation did not interrupt the stream")
            .expect("stream task panicked");
        assert!(sink
            .events()
            .iter()
            .any(|event| matches!(event, TestEvent::Done { reason, .. } if *reason == "cancelled")));
        server.abort();
    }

    #[tokio::test]
    async fn rate_limits_include_retry_hint() {
        let body = br#"{"error":{"message":"slow down"}}"#.to_vec();
        let (url, server) = spawn_server(MockResponse {
            status: "429 Too Many Requests",
            headers: vec![
                ("Content-Type", "application/json".into()),
                ("Retry-After", "7".into()),
            ],
            body: body.clone(),
            chunk_delay: None,
            content_length: Some(body.len()),
        })
        .await;

        let sink = TestSink::new();
        tokio::time::timeout(
            Duration::from_secs(5),
            stream_chat(
                sink.clone(),
                reqwest::Client::new(),
                provider_at("openai", &url),
                request_with_id("limited"),
                Arc::new(AtomicBool::new(false)),
            ),
        )
        .await
        .expect("stream timed out");

        let message = sink
            .events()
            .into_iter()
            .find_map(|event| match event {
                TestEvent::Error { message, .. } => Some(message),
                _ => None,
            })
            .expect("expected an error event");
        assert!(message.contains("Rate limited"));
        assert!(message.contains("slow down"));
        assert!(message.contains("retrying after 7"));
        server.abort();
    }


    #[tokio::test]
    async fn interrupted_bodies_surface_an_error() {
        let body = sse_body(&[r#"{"choices":[{"delta":{"content":"partial"}}]}"#]);
        let (url, server) = spawn_server(MockResponse {
            status: "200 OK",
            headers: vec![("Content-Type", "text/event-stream".into())],
            body,
            chunk_delay: None,
            content_length: Some(4096),
        })
        .await;

        let sink = TestSink::new();
        tokio::time::timeout(
            Duration::from_secs(5),
            stream_chat(
                sink.clone(),
                reqwest::Client::new(),
                provider_at("openai", &url),
                request_with_id("cut-off"),
                Arc::new(AtomicBool::new(false)),
            ),
        )
        .await
        .expect("stream timed out");

        assert!(sink.events().iter().any(|event| matches!(
            event,
            TestEvent::Error { message, .. } if message.contains("stream ended unexpectedly")
        )));
        server.abort();
    }

    #[tokio::test]
    async fn server_errors_are_explained() {
        let body = b"upstream exploded".to_vec();
        let (url, server) = spawn_server(MockResponse {
            status: "500 Internal Server Error",
            headers: vec![("Content-Type", "text/plain".into())],
            body: body.clone(),
            chunk_delay: None,
            content_length: Some(body.len()),
        })
        .await;

        let sink = TestSink::new();
        tokio::time::timeout(
            Duration::from_secs(5),
            stream_chat(
                sink.clone(),
                reqwest::Client::new(),
                provider_at("openai", &url),
                request_with_id("server-error"),
                Arc::new(AtomicBool::new(false)),
            ),
        )
        .await
        .expect("stream timed out");

        let message = sink
            .events()
            .into_iter()
            .find_map(|event| match event {
                TestEvent::Error { message, .. } => Some(message),
                _ => None,
            })
            .expect("expected an error event");
        assert!(message.contains("server error"));
        assert!(message.contains("upstream exploded"));
        server.abort();
    }

    #[tokio::test]
    async fn concurrent_streams_keep_their_ids_isolated() {
        let (url_a, server_a) = spawn_server(MockResponse {
            status: "200 OK",
            headers: vec![("Content-Type", "text/event-stream".into())],
            body: sse_body(&[
                r#"{"choices":[{"delta":{"content":"AAA"}}]}"#,
                "[DONE]",
            ]),
            chunk_delay: None,
            content_length: None,
        })
        .await;
        let (url_b, server_b) = spawn_server(MockResponse {
            status: "200 OK",
            headers: vec![("Content-Type", "text/event-stream".into())],
            body: sse_body(&[
                r#"{"choices":[{"delta":{"content":"BBB"}}]}"#,
                "[DONE]",
            ]),
            chunk_delay: None,
            content_length: None,
        })
        .await;

        let sink_a = TestSink::new();
        let sink_b = TestSink::new();
        let client = reqwest::Client::new();
        let (_a, _b) = tokio::join!(
            stream_chat(
                sink_a.clone(),
                client.clone(),
                provider_at("openai", &url_a),
                request_with_id("id-a"),
                Arc::new(AtomicBool::new(false)),
            ),
            stream_chat(
                sink_b.clone(),
                client.clone(),
                provider_at("openai", &url_b),
                request_with_id("id-b"),
                Arc::new(AtomicBool::new(false)),
            ),
        );

        let events_a = sink_a.events();
        let events_b = sink_b.events();
        assert!(!events_a.is_empty() && !events_b.is_empty());
        assert!(events_a.iter().all(|event| match event {
            TestEvent::Delta { id, .. }
            | TestEvent::Done { id, .. }
            | TestEvent::Error { id, .. } => id.as_str() == "id-a",
        }));
        assert!(events_b.iter().all(|event| match event {
            TestEvent::Delta { id, .. }
            | TestEvent::Done { id, .. }
            | TestEvent::Error { id, .. } => id.as_str() == "id-b",
        }));
        let text_a: String = events_a
            .iter()
            .filter_map(|event| match event {
                TestEvent::Delta { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        let text_b: String = events_b
            .iter()
            .filter_map(|event| match event {
                TestEvent::Delta { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text_a, "AAA");
        assert_eq!(text_b, "BBB");
        server_a.abort();
        server_b.abort();
    }


    #[tokio::test]
    async fn clean_eof_without_openai_done_is_accepted() {
        let body = sse_body(&[r#"{"choices":[{"delta":{"content":"done"}}]}"#]);
        let (url, server) = spawn_server(MockResponse {
            status: "200 OK",
            headers: vec![("Content-Type", "text/event-stream".into())],
            body,
            chunk_delay: None,
            content_length: None,
        })
        .await;

        let sink = TestSink::new();
        tokio::time::timeout(
            Duration::from_secs(5),
            stream_chat(
                sink.clone(),
                reqwest::Client::new(),
                provider_at("openai", &url),
                request_with_id("no-sentinel"),
                Arc::new(AtomicBool::new(false)),
            ),
        )
        .await
        .expect("stream timed out");

        assert!(sink
            .events()
            .iter()
            .any(|event| matches!(event, TestEvent::Done { reason, .. } if *reason == "stop")));
        server.abort();
    }

    #[tokio::test]
    async fn empty_streams_are_reported_as_errors() {
        let (url, server) = spawn_server(MockResponse {
            status: "200 OK",
            headers: vec![("Content-Type", "text/event-stream".into())],
            body: Vec::new(),
            chunk_delay: None,
            content_length: None,
        })
        .await;

        let sink = TestSink::new();
        tokio::time::timeout(
            Duration::from_secs(5),
            stream_chat(
                sink.clone(),
                reqwest::Client::new(),
                provider_at("openai", &url),
                request_with_id("empty"),
                Arc::new(AtomicBool::new(false)),
            ),
        )
        .await
        .expect("stream timed out");

        assert!(sink.events().iter().any(|event| matches!(
            event,
            TestEvent::Error { message, .. } if message.contains("without sending a response")
        )));
        server.abort();
    }

    #[tokio::test]
    async fn a_pre_cancelled_request_never_hits_the_provider() {
        let sink = TestSink::new();
        let cancel = Arc::new(AtomicBool::new(true));
        let started = std::time::Instant::now();
        tokio::time::timeout(
            Duration::from_secs(2),
            stream_chat(
                sink.clone(),
                reqwest::Client::new(),
                provider_at("openai", "http://127.0.0.1:1"),
                request_with_id("pre-cancel"),
                cancel,
            ),
        )
        .await
        .expect("pre-cancelled request hung");

        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(sink
            .events()
            .iter()
            .any(|event| matches!(event, TestEvent::Done { reason, .. } if *reason == "cancelled")));
    }

}

