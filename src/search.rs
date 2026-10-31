//! Optional web search for the assistant.
//!
//! Search is bring-your-own-key and disabled unless the user configures a
//! provider. Results are untrusted data: HTML is stripped, URLs are restricted
//! to http(s), and the prompt explicitly tells the model not to follow
//! instructions found in retrieved pages.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, Method, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::credentials::Credentials;
use crate::settings::{is_local_endpoint, search_credential_id, SearchProviderConfig, Settings};

const MAX_QUERY_CHARS: usize = 240;
const MAX_TITLE_CHARS: usize = 180;
const MAX_SNIPPET_CHARS: usize = 700;
const MAX_URL_CHARS: usize = 2_048;
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchError {
    /// `config`, `auth`, `rate_limit`, `timeout`, `network`, `cancelled` or
    /// `provider`.
    pub kind: String,
    pub message: String,
}

impl SearchError {
    pub fn new(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.to_string(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedSearch {
    pub provider: SearchProviderConfig,
    pub key: String,
    pub max_results: u32,
    pub timeout_secs: u64,
}

/// Resolves the active search provider and its key immediately before a chat
/// request. The key never enters the webview or `settings.json`.
pub fn resolve(settings: &Settings, credentials: &Credentials) -> Result<ResolvedSearch, String> {
    let config = &settings.web_search;
    if !config.enabled {
        return Err(
            "Web search is turned off. Enable it in Settings and add a search provider.".into(),
        );
    }
    let provider = config
        .provider(&config.active_provider)
        .cloned()
        .or_else(|| config.providers.first().cloned())
        .ok_or_else(|| {
            "No web-search provider is configured. Open Settings (Ctrl+,).".to_string()
        })?;
    let key = credentials
        .get(&search_credential_id(&provider.id))?
        .unwrap_or_default();
    if requires_key(&provider) && key.trim().is_empty() {
        return Err(format!(
            "{} needs an API key before web search can run. Open Settings (Ctrl+,).",
            provider.label
        ));
    }
    if provider.endpoint.trim().is_empty() {
        return Err(format!(
            "{} has no endpoint. Open Settings (Ctrl+,) and add one.",
            provider.label
        ));
    }
    Ok(ResolvedSearch {
        provider,
        key,
        max_results: config.max_results.clamp(1, 10),
        timeout_secs: config.timeout_secs.clamp(5, 60),
    })
}

/// SearXNG is commonly self-hosted without authentication. Any provider on a
/// loopback endpoint is also allowed to omit a key.
pub fn requires_key(provider: &SearchProviderConfig) -> bool {
    provider.kind != "searxng" && !is_local_endpoint(&provider.endpoint)
}

#[derive(Debug)]
pub struct PreparedSearchRequest {
    pub method: Method,
    pub url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Option<Value>,
}

fn endpoint_base(provider: &SearchProviderConfig) -> Result<Url, SearchError> {
    let raw = provider.endpoint.trim();
    if raw.is_empty() {
        return Err(SearchError::new(
            "config",
            format!(
                "{} has no endpoint. Open Settings (Ctrl+,).",
                provider.label
            ),
        ));
    }
    let mut url = Url::parse(raw).map_err(|err| {
        SearchError::new(
            "config",
            format!("The {} endpoint is not a valid URL: {err}", provider.label),
        )
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(SearchError::new(
            "config",
            "Search endpoints must use http:// or https://.",
        ));
    }
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err(SearchError::new(
            "config",
            "The search endpoint must name a host and must not contain credentials.",
        ));
    }
    // A bare provider origin should get the provider's usual API path.
    if url.path().is_empty() || url.path() == "/" {
        let path = match provider.kind.as_str() {
            "brave" => "/res/v1/web/search",
            _ => "/search",
        };
        url.set_path(path);
    }
    Ok(url)
}

/// Builds a provider request without sending it. Kept separate so provider
/// payloads and auth headers can be unit-tested without network access.
pub fn build_search_request(
    provider: &SearchProviderConfig,
    key: &str,
    query: &str,
    max_results: u32,
) -> Result<PreparedSearchRequest, SearchError> {
    let query = sanitize_query(query)
        .ok_or_else(|| SearchError::new("config", "The search query is empty."))?;
    let max_results = max_results.clamp(1, 10);
    let key = key.trim();
    if requires_key(provider) && key.is_empty() {
        return Err(SearchError::new(
            "auth",
            format!("{} needs an API key.", provider.label),
        ));
    }
    let mut url = endpoint_base(provider)?;
    let mut headers = vec![("accept".to_string(), "application/json".to_string())];
    let body;

    match provider.kind.as_str() {
        "tavily" => {
            headers.push(("authorization".into(), format!("Bearer {key}")));
            headers.push(("content-type".into(), "application/json".into()));
            body = Some(json!({
                "query": query,
                "max_results": max_results,
                "search_depth": "basic",
                "include_answer": false,
                "include_raw_content": false,
            }));
        }
        "brave" => {
            headers.push(("x-subscription-token".into(), key.to_string()));
            add_query(&mut url, "q", &query);
            add_query(&mut url, "count", &max_results.to_string());
            body = None;
        }
        "serper" => {
            headers.push(("x-api-key".into(), key.to_string()));
            headers.push(("content-type".into(), "application/json".into()));
            body = Some(json!({ "q": query, "num": max_results }));
        }
        "exa" => {
            headers.push(("x-api-key".into(), key.to_string()));
            headers.push(("content-type".into(), "application/json".into()));
            body = Some(json!({
                "query": query,
                "numResults": max_results,
                "type": "auto",
                "contents": { "text": { "maxCharacters": 900 } }
            }));
        }
        "searxng" => {
            if !key.is_empty() {
                headers.push(("authorization".into(), format!("Bearer {key}")));
            }
            add_query(&mut url, "q", &query);
            add_query(&mut url, "format", "json");
            add_query(&mut url, "safesearch", "1");
            add_query(&mut url, "language", "en");
            body = None;
        }
        other => {
            return Err(SearchError::new(
                "config",
                format!("Unknown search provider type: {other}"),
            ));
        }
    }

    Ok(PreparedSearchRequest {
        method: if body.is_some() {
            Method::POST
        } else {
            Method::GET
        },
        url,
        headers,
        body,
    })
}

/// Strips tags and scripts, decodes common HTML entities, removes control
/// characters and collapses whitespace. Returned text is safe to render with
/// `textContent` and is treated as untrusted context by the model.
pub fn sanitize_external_text(input: &str, max_chars: usize) -> String {
    let mut without_tags = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => without_tags.push(ch),
            _ => {}
        }
    }

    let decoded = decode_entities(&without_tags);
    let cleaned: String = decoded
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = collapsed.chars().take(max_chars).collect();
    if collapsed.chars().count() > max_chars {
        out.push('…');
    }
    out
}

fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '&' {
            let rest: String = chars[index..].iter().take(12).collect();
            let mut consumed = 0;
            let replacement = if rest.starts_with("&amp;") {
                consumed = 5;
                Some('&')
            } else if rest.starts_with("&lt;") {
                consumed = 4;
                Some('<')
            } else if rest.starts_with("&gt;") {
                consumed = 4;
                Some('>')
            } else if rest.starts_with("&quot;") {
                consumed = 6;
                Some('"')
            } else if rest.starts_with("&#39;") || rest.starts_with("&apos;") {
                consumed = if rest.starts_with("&#39;") { 5 } else { 6 };
                Some('\'')
            } else if rest.starts_with("&nbsp;") {
                consumed = 6;
                Some(' ')
            } else {
                None
            };
            if let Some(replacement) = replacement {
                out.push(replacement);
                index += consumed;
                continue;
            }
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

pub fn safe_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.chars().any(char::is_control) || raw.len() > MAX_URL_CHARS {
        return None;
    }
    let parsed = Url::parse(raw).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    if parsed.host_str().is_none() || !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    Some(parsed.to_string())
}

fn add_query(url: &mut Url, key: &str, value: &str) {
    url.query_pairs_mut().append_pair(key, value);
}

pub fn sanitize_query(query: &str) -> Option<String> {
    let cleaned: String = query
        .chars()
        .filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t')
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed: String = cleaned.chars().take(MAX_QUERY_CHARS).collect();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Normalizes provider-specific JSON into the common result shape.
pub fn parse_results(kind: &str, value: &Value, provider_label: &str) -> Vec<WebSearchHit> {
    let mut hits: Vec<WebSearchHit> = Vec::new();
    let mut push = |title: Option<&str>, url: Option<&str>, snippet: Option<&str>| {
        let Some(url) = url.and_then(safe_url) else {
            return;
        };
        if hits.iter().any(|hit| hit.url == url) {
            return;
        }
        let title = sanitize_external_text(title.unwrap_or_default(), MAX_TITLE_CHARS);
        let snippet = sanitize_external_text(snippet.unwrap_or_default(), MAX_SNIPPET_CHARS);
        hits.push(WebSearchHit {
            title: if title.is_empty() {
                "Untitled result".to_string()
            } else {
                title
            },
            url,
            snippet,
            provider: provider_label.to_string(),
        });
    };

    match kind {
        "brave" => {
            for item in value["web"]["results"]
                .as_array()
                .cloned()
                .unwrap_or_default()
            {
                push(
                    item["title"].as_str(),
                    item["url"].as_str(),
                    item["description"].as_str(),
                );
            }
        }
        "serper" => {
            for item in value["organic"].as_array().cloned().unwrap_or_default() {
                push(
                    item["title"].as_str(),
                    item["link"].as_str(),
                    item["snippet"].as_str(),
                );
            }
        }
        "exa" => {
            for item in value["results"].as_array().cloned().unwrap_or_default() {
                let snippet = item["text"].as_str().or_else(|| {
                    item["highlights"]
                        .as_array()
                        .and_then(|parts| parts.first())
                        .and_then(Value::as_str)
                });
                push(item["title"].as_str(), item["url"].as_str(), snippet);
            }
        }
        "tavily" => {
            for item in value["results"].as_array().cloned().unwrap_or_default() {
                push(
                    item["title"].as_str(),
                    item["url"].as_str(),
                    item["content"].as_str(),
                );
            }
        }
        "searxng" => {
            for item in value["results"].as_array().cloned().unwrap_or_default() {
                push(
                    item["title"].as_str(),
                    item["url"].as_str(),
                    item["content"].as_str(),
                );
            }
        }
        _ => {}
    }
    hits
}

/// Adds new results while de-duplicating by URL and enforcing a total cap.
pub fn merge_hits(existing: &mut Vec<WebSearchHit>, incoming: Vec<WebSearchHit>, limit: usize) {
    for hit in incoming {
        if existing.len() >= limit {
            break;
        }
        if existing.iter().any(|known| known.url == hit.url) {
            continue;
        }
        existing.push(hit);
    }
}

/// Stable numbered context for the model. Snippets are already sanitized and
/// explicitly labelled as untrusted.
pub fn format_results_for_prompt(hits: &[WebSearchHit]) -> String {
    let mut out = String::new();
    for (index, hit) in hits.iter().enumerate() {
        out.push_str(&format!(
            "[{}] {}\nURL: {}\nExcerpt: {}\n\n",
            index + 1,
            hit.title,
            hit.url,
            hit.snippet
        ));
    }
    out
}

fn cancelled_error() -> SearchError {
    SearchError::new("cancelled", "Search cancelled.")
}

pub fn http_error(status: StatusCode, retry_after: Option<&str>, body: &str) -> SearchError {
    let detail = sanitize_external_text(body, 300);
    let mut message = match status.as_u16() {
        401 | 403 => format!("Search authentication failed ({status}). Check the search API key."),
        408 | 504 => format!("The search provider timed out ({status}). Try again."),
        429 => format!("The search provider rate-limited this request ({status})."),
        500..=599 => format!("The search provider had a server error ({status})."),
        _ => format!("The search provider returned an error ({status})."),
    };
    if !detail.is_empty() {
        message.push(' ');
        message.push_str(&detail);
    }
    if let Some(retry) = retry_after {
        message.push_str(&format!(" Retry after {retry}."));
    }
    let kind = match status.as_u16() {
        401 | 403 => "auth",
        408 | 504 => "timeout",
        429 => "rate_limit",
        _ => "provider",
    };
    SearchError::new(kind, message)
}

async fn wait_for_cancel(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Executes one search query with a hard timeout and cancellation. Provider
/// credentials are sent only in headers (except SearXNG's optional bearer).
pub async fn search_web(
    client: &Client,
    resolved: &ResolvedSearch,
    query: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<WebSearchHit>, SearchError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(cancelled_error());
    }
    let prepared = build_search_request(
        &resolved.provider,
        &resolved.key,
        query,
        resolved.max_results,
    )?;

    let mut builder = client.request(prepared.method, prepared.url.clone());
    for (name, value) in &prepared.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    if let Some(body) = &prepared.body {
        builder = builder.json(body);
    }

    let timeout = Duration::from_secs(resolved.timeout_secs.clamp(5, 60));
    let response = tokio::select! {
        biased;
        _ = wait_for_cancel(cancel) => return Err(cancelled_error()),
        result = tokio::time::timeout(timeout, builder.send()) => match result {
            Err(_) => {
                return Err(SearchError::new(
                    "timeout",
                    format!("{} did not respond within {} seconds.", resolved.provider.label, timeout.as_secs()),
                ))
            }
            Ok(Err(err)) => {
                return Err(SearchError::new(
                    "network",
                    format!("Could not reach {}: {err}", resolved.provider.label),
                ))
            }
            Ok(Ok(response)) => response,
        },
    };

    let status = response.status();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES)
    {
        return Err(SearchError::new(
            "provider",
            format!(
                "{} returned an unexpectedly large response.",
                resolved.provider.label
            ),
        ));
    }

    let body = tokio::select! {
        biased;
        _ = wait_for_cancel(cancel) => return Err(cancelled_error()),
        result = tokio::time::timeout(timeout, response.text()) => match result {
            Err(_) => {
                return Err(SearchError::new(
                    "timeout",
                    format!("{} took too long to return results.", resolved.provider.label),
                ))
            }
            Ok(Err(err)) => {
                return Err(SearchError::new(
                    "network",
                    format!("Could not read the search response: {err}"),
                ))
            }
            Ok(Ok(body)) => body,
        },
    };

    if !status.is_success() {
        return Err(http_error(status, retry_after.as_deref(), &body));
    }

    let value: Value = serde_json::from_str(&body).map_err(|err| {
        SearchError::new(
            "provider",
            format!(
                "{} returned an unexpected response: {err}",
                resolved.provider.label
            ),
        )
    })?;
    let mut hits = parse_results(&resolved.provider.kind, &value, &resolved.provider.label);
    let mut merged = Vec::new();
    merge_hits(
        &mut merged,
        std::mem::take(&mut hits),
        resolved.max_results as usize,
    );
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn provider(kind: &str, endpoint: &str) -> SearchProviderConfig {
        SearchProviderConfig {
            id: kind.into(),
            label: kind.into(),
            kind: kind.into(),
            endpoint: endpoint.into(),
            api_key: String::new(),
        }
    }

    #[test]
    fn builds_tavily_post_with_bearer_auth() {
        let request = build_search_request(
            &provider("tavily", "https://api.tavily.com/search"),
            "tvly-test",
            "rust news",
            4,
        )
        .unwrap();
        assert_eq!(request.method, Method::POST);
        assert_eq!(request.url.as_str(), "https://api.tavily.com/search");
        assert!(request
            .headers
            .iter()
            .any(|(name, value)| name == "authorization" && value == "Bearer tvly-test"));
        let body = request.body.unwrap();
        assert_eq!(body["query"], "rust news");
        assert_eq!(body["max_results"], 4);
    }

    #[test]
    fn builds_brave_and_searxng_queries() {
        let brave = build_search_request(
            &provider("brave", "https://api.search.brave.com/res/v1/web/search"),
            "brave-key",
            "inkwell markdown",
            3,
        )
        .unwrap();
        assert_eq!(brave.method, Method::GET);
        assert_eq!(
            brave
                .url
                .query_pairs()
                .find(|(key, _)| key == "q")
                .unwrap()
                .1,
            "inkwell markdown"
        );
        assert!(brave
            .headers
            .iter()
            .any(|(name, value)| name == "x-subscription-token" && value == "brave-key"));

        let searx = build_search_request(
            &provider("searxng", "http://localhost:8080/search"),
            "",
            "local search",
            2,
        )
        .unwrap();
        assert_eq!(searx.method, Method::GET);
        assert_eq!(searx.url.host_str(), Some("localhost"));
        assert_eq!(
            searx
                .url
                .query_pairs()
                .find(|(key, _)| key == "format")
                .unwrap()
                .1,
            "json"
        );
    }

    #[test]
    fn parses_provider_payloads_into_common_hits() {
        let tavily = json!({
            "results": [{
                "title": "<b>Rust</b> news",
                "url": "https://example.com/rust",
                "content": "<script>ignore()</script>Latest &amp; greatest"
            }]
        });
        let hits = parse_results("tavily", &tavily, "Tavily");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Rust news");
        assert_eq!(hits[0].snippet, "ignore()Latest & greatest");
        assert_eq!(hits[0].url, "https://example.com/rust");

        let brave = json!({
            "web": { "results": [{ "title": "Brave", "url": "https://brave.example", "description": "Desc" }] }
        });
        assert_eq!(parse_results("brave", &brave, "Brave")[0].title, "Brave");

        let serper = json!({
            "organic": [{ "title": "Serper", "link": "https://serper.example", "snippet": "Snip" }]
        });
        assert_eq!(
            parse_results("serper", &serper, "Serper")[0].snippet,
            "Snip"
        );

        let exa = json!({
            "results": [{ "title": "Exa", "url": "https://exa.example", "highlights": ["Highlight"] }]
        });
        assert_eq!(parse_results("exa", &exa, "Exa")[0].snippet, "Highlight");

        let searx = json!({
            "results": [{ "title": "SearX", "url": "https://searx.example", "content": "Content" }]
        });
        assert_eq!(
            parse_results("searxng", &searx, "SearXNG")[0].provider,
            "SearXNG"
        );
    }

    #[test]
    fn sanitizes_external_text_and_rejects_unsafe_urls() {
        let cleaned = sanitize_external_text("<p>Hello &amp; <b>world</b>\u{0007}</p>", 100);
        assert_eq!(cleaned, "Hello & world");
        assert!(safe_url("https://example.com/a").is_some());
        assert!(safe_url("http://localhost:8080").is_some());
        assert!(safe_url("javascript:alert(1)").is_none());
        assert!(safe_url("file:///etc/passwd").is_none());
        assert!(safe_url("https://user:pass@example.com").is_none());
        assert!(safe_url("https://example.com/\r\nX: y").is_none());
    }

    #[test]
    fn merges_results_by_url_and_enforces_a_limit() {
        let hit = |url: &str| WebSearchHit {
            title: "Title".into(),
            url: url.into(),
            snippet: String::new(),
            provider: "Test".into(),
        };
        let mut hits = vec![hit("https://a.example")];
        merge_hits(
            &mut hits,
            vec![
                hit("https://a.example"),
                hit("https://b.example"),
                hit("https://c.example"),
            ],
            2,
        );
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[1].url, "https://b.example");
    }

    #[test]
    fn resolve_requires_enabled_config_and_secret() {
        let credentials = Credentials::memory_only();
        let mut settings = Settings::default();
        assert!(resolve(&settings, &credentials).is_err());

        settings.web_search.enabled = true;
        assert!(resolve(&settings, &credentials).is_err());

        credentials.set("search:tavily", "tvly-secret").unwrap();
        let resolved = resolve(&settings, &credentials).unwrap();
        assert_eq!(resolved.provider.id, "tavily");
        assert_eq!(resolved.key, "tvly-secret");
        assert_eq!(resolved.max_results, 5);
    }

    #[test]
    fn disabled_cancel_short_circuits_network_access() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let resolved = ResolvedSearch {
                provider: provider("serper", "http://127.0.0.1:1/search"),
                key: "key".into(),
                max_results: 3,
                timeout_secs: 5,
            };
            let cancel = Arc::new(AtomicBool::new(true));
            let err = search_web(&Client::new(), &resolved, "query", &cancel)
                .await
                .unwrap_err();
            assert_eq!(err.kind, "cancelled");
        });
    }

    #[tokio::test]
    async fn surfaces_rate_limits_with_retry_hint() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 2048];
            let _ = socket.read(&mut buffer).await;
            let body = "{\"message\":\"quota exceeded\"}";
            let response = format!(
                "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nRetry-After: 12\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let resolved = ResolvedSearch {
            provider: provider("serper", &format!("http://{address}/search")),
            key: "key".into(),
            max_results: 2,
            timeout_secs: 5,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let error = search_web(&Client::new(), &resolved, "query", &cancel)
            .await
            .unwrap_err();
        assert_eq!(error.kind, "rate_limit");
        assert!(error.message.contains("Retry after 12"));
        server.await.unwrap();
    }
}
