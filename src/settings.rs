//! Persistent settings.
//!
//! `settings.json` holds only non-secret configuration (providers, models,
//! endpoints, prompts, preferences). API keys live in the operating system
//! credential store via [`crate::credentials`] and are never sent back to the
//! webview: the frontend only receives `hasKey` flags.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::credentials::{CredentialStatus, Credentials};

pub const DEFAULT_SYSTEM_PROMPT: &str = "You are Inkwell, a concise and precise reading assistant embedded in a Markdown viewer. \
Answer questions about the document the user is reading. Prefer Markdown in your replies, quote short snippets when useful, \
and say when the document does not contain the answer.";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub label: String,
    /// Transport family: `openai`, `anthropic` or `gemini`.
    pub kind: String,
    pub base_url: String,
    /// Legacy plaintext key from older settings files. It is read once for
    /// migration and never serialized back to disk.
    #[serde(default, skip_serializing)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchProviderConfig {
    pub id: String,
    pub label: String,
    /// Transport family: `tavily`, `brave`, `serper`, `exa` or `searxng`.
    pub kind: String,
    pub endpoint: String,
    /// Legacy plaintext key from older settings files. It is read once for
    /// migration and never serialized back to disk.
    #[serde(default, skip_serializing)]
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_search_provider")]
    pub active_provider: String,
    #[serde(default = "default_search_providers")]
    pub providers: Vec<SearchProviderConfig>,
    #[serde(default = "default_search_max_results")]
    pub max_results: u32,
    #[serde(default = "default_search_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            active_provider: default_search_provider(),
            providers: default_search_providers(),
            max_results: default_search_max_results(),
            timeout_secs: default_search_timeout_secs(),
        }
    }
}

impl WebSearchConfig {
    pub fn provider(&self, id: &str) -> Option<&SearchProviderConfig> {
        self.providers.iter().find(|provider| provider.id == id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub active_provider: String,
    pub providers: Vec<ProviderConfig>,
    pub temperature: f64,
    pub max_tokens: u32,
    pub system_prompt: String,
    pub include_document: bool,
    #[serde(default)]
    pub thinking: bool,
    #[serde(default)]
    pub web_search: WebSearchConfig,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            active_provider: "openai".into(),
            providers: default_providers(),
            temperature: 0.3,
            max_tokens: 1500,
            system_prompt: DEFAULT_SYSTEM_PROMPT.into(),
            include_document: true,
            thinking: false,
            web_search: WebSearchConfig::default(),
        }
    }
}

pub fn default_providers() -> Vec<ProviderConfig> {
    let preset = |id: &str, label: &str, kind: &str, base: &str, model: &str| ProviderConfig {
        id: id.into(),
        label: label.into(),
        kind: kind.into(),
        base_url: base.into(),
        api_key: String::new(),
        model: model.into(),
    };
    vec![
        preset(
            "openai",
            "OpenAI",
            "openai",
            "https://api.openai.com/v1",
            "gpt-4o-mini",
        ),
        preset(
            "anthropic",
            "Anthropic",
            "anthropic",
            "https://api.anthropic.com",
            "claude-3-5-sonnet-latest",
        ),
        preset(
            "gemini",
            "Google Gemini",
            "gemini",
            "https://generativelanguage.googleapis.com",
            "gemini-2.0-flash",
        ),
        preset(
            "groq",
            "Groq",
            "openai",
            "https://api.groq.com/openai/v1",
            "llama-3.3-70b-versatile",
        ),
        preset(
            "mistral",
            "Mistral",
            "openai",
            "https://api.mistral.ai/v1",
            "mistral-large-latest",
        ),
        preset(
            "deepseek",
            "DeepSeek",
            "openai",
            "https://api.deepseek.com/v1",
            "deepseek-chat",
        ),
        preset(
            "openrouter",
            "OpenRouter",
            "openai",
            "https://openrouter.ai/api/v1",
            "anthropic/claude-3.5-sonnet",
        ),
        preset(
            "xai",
            "xAI",
            "openai",
            "https://api.x.ai/v1",
            "grok-2-latest",
        ),
        preset(
            "ollama",
            "Ollama (local)",
            "openai",
            "http://localhost:11434/v1",
            "llama3.2",
        ),
        preset(
            "lmstudio",
            "LM Studio (local)",
            "openai",
            "http://localhost:1234/v1",
            "local-model",
        ),
        preset("custom", "Custom endpoint", "openai", "", ""),
    ]
}
pub fn default_search_providers() -> Vec<SearchProviderConfig> {
    let preset = |id: &str, label: &str, kind: &str, endpoint: &str| SearchProviderConfig {
        id: id.into(),
        label: label.into(),
        kind: kind.into(),
        endpoint: endpoint.into(),
        api_key: String::new(),
    };
    vec![
        preset(
            "tavily",
            "Tavily",
            "tavily",
            "https://api.tavily.com/search",
        ),
        preset(
            "brave",
            "Brave Search",
            "brave",
            "https://api.search.brave.com/res/v1/web/search",
        ),
        preset(
            "serper",
            "Serper (Google)",
            "serper",
            "https://google.serper.dev/search",
        ),
        preset("exa", "Exa", "exa", "https://api.exa.ai/search"),
        preset(
            "searxng",
            "SearXNG (self-hosted)",
            "searxng",
            "http://localhost:8080/search",
        ),
    ]
}

pub fn default_search_provider() -> String {
    "tavily".into()
}

fn default_search_max_results() -> u32 {
    5
}

fn default_search_timeout_secs() -> u64 {
    15
}

/// Credential-store namespace for web search keys. Kept separate from chat
/// provider ids so a provider named "brave" cannot collide with a search
/// provider of the same name.
pub fn search_credential_id(provider_id: &str) -> String {
    format!("search:{provider_id}")
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchProviderView {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub endpoint: String,
    pub has_key: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchView {
    pub enabled: bool,
    pub active_provider: String,
    pub providers: Vec<SearchProviderView>,
    pub max_results: u32,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub active_provider: String,
    pub providers: Vec<ProviderView>,
    pub temperature: f64,
    pub max_tokens: u32,
    pub system_prompt: String,
    pub include_document: bool,
    pub thinking: bool,
    pub web_search: WebSearchView,
    /// Whether the OS credential store is available, and which backend it is.
    pub credentials: CredentialStatus,
}

impl Settings {
    pub fn view(&self, credentials: &Credentials) -> SettingsView {
        SettingsView {
            active_provider: self.active_provider.clone(),
            providers: self
                .providers
                .iter()
                .map(|p| ProviderView {
                    id: p.id.clone(),
                    label: p.label.clone(),
                    kind: p.kind.clone(),
                    base_url: p.base_url.clone(),
                    model: p.model.clone(),
                    has_key: credentials.has(&p.id),
                })
                .collect(),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            system_prompt: self.system_prompt.clone(),
            include_document: self.include_document,
            thinking: self.thinking,
            web_search: WebSearchView {
                enabled: self.web_search.enabled,
                active_provider: self.web_search.active_provider.clone(),
                providers: self
                    .web_search
                    .providers
                    .iter()
                    .map(|p| SearchProviderView {
                        id: p.id.clone(),
                        label: p.label.clone(),
                        kind: p.kind.clone(),
                        endpoint: p.endpoint.clone(),
                        has_key: credentials.has(&search_credential_id(&p.id)),
                    })
                    .collect(),
                max_results: self.web_search.max_results,
                timeout_secs: self.web_search.timeout_secs,
            },
            credentials: credentials.status().clone(),
        }
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|p| p.id == id)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    pub id: String,
    #[serde(default)]
    pub label: String,
    pub kind: String,
    pub base_url: String,
    pub model: String,
    /// `None` keeps the stored key, `Some("")` clears it.
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchProviderInput {
    pub id: String,
    #[serde(default)]
    pub label: String,
    pub kind: String,
    pub endpoint: String,
    /// `None` keeps the stored key, `Some("")` clears it.
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchInput {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub active_provider: String,
    #[serde(default)]
    pub providers: Vec<SearchProviderInput>,
    #[serde(default = "default_search_max_results")]
    pub max_results: u32,
    #[serde(default = "default_search_timeout_secs")]
    pub timeout_secs: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub active_provider: String,
    pub providers: Vec<ProviderInput>,
    pub temperature: f64,
    pub max_tokens: u32,
    pub system_prompt: String,
    pub include_document: bool,
    #[serde(default)]
    pub thinking: bool,
    /// `None` preserves the existing web-search configuration. Older payloads
    /// and older settings files omit this field.
    #[serde(default)]
    pub web_search: Option<WebSearchInput>,
}

impl SettingsInput {
    pub fn merge(self, mut previous: Settings) -> Settings {
        let merged: Vec<ProviderConfig> = self
            .providers
            .into_iter()
            .filter(|p| !p.id.trim().is_empty())
            .map(|p| ProviderConfig {
                id: p.id,
                label: p.label,
                kind: p.kind,
                base_url: p.base_url.trim().trim_end_matches('/').to_string(),
                model: p.model.trim().to_string(),
                api_key: String::new(),
            })
            .collect();

        let providers = if merged.is_empty() {
            previous.providers.clone()
        } else {
            merged
        };
        let active = if providers.iter().any(|p| p.id == self.active_provider) {
            self.active_provider
        } else {
            providers[0].id.clone()
        };

        previous.providers = providers;
        previous.active_provider = active;
        previous.temperature = self.temperature.clamp(0.0, 2.0);
        previous.max_tokens = self.max_tokens.clamp(64, 32_000);
        previous.system_prompt = self.system_prompt;
        previous.include_document = self.include_document;
        previous.thinking = self.thinking;
        if let Some(input) = self.web_search {
            previous.web_search = merge_web_search(input, previous.web_search);
        }
        previous
    }
}

fn merge_web_search(input: WebSearchInput, previous: WebSearchConfig) -> WebSearchConfig {
    let providers: Vec<SearchProviderConfig> = input
        .providers
        .into_iter()
        .filter(|provider| !provider.id.trim().is_empty())
        .map(|provider| SearchProviderConfig {
            id: provider.id.trim().to_string(),
            label: provider.label.trim().to_string(),
            kind: provider.kind.trim().to_ascii_lowercase(),
            endpoint: provider.endpoint.trim().trim_end_matches('/').to_string(),
            api_key: String::new(),
        })
        .collect();

    let providers = if providers.is_empty() {
        previous.providers.clone()
    } else {
        providers
    };
    let active_provider = if providers
        .iter()
        .any(|provider| provider.id == input.active_provider)
    {
        input.active_provider
    } else {
        providers[0].id.clone()
    };

    WebSearchConfig {
        enabled: input.enabled,
        active_provider,
        providers,
        max_results: input.max_results.clamp(1, 10),
        timeout_secs: input.timeout_secs.clamp(5, 60),
    }
}

pub fn settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Inkwell")
        .join("settings.json")
}

pub fn load() -> Settings {
    let path = settings_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return Settings::default();
    };
    match serde_json::from_str::<Settings>(&raw) {
        Ok(settings) => settings,
        Err(err) => {
            eprintln!("inkwell: settings.json is invalid ({err}); using defaults");
            Settings::default()
        }
    }
}

pub fn save(credentials: &Credentials, incoming: SettingsInput) -> Result<SettingsView, String> {
    // Persist secrets first: if the credential store rejects a key, do not
    // commit a provider configuration that claims to have it.
    for provider in &incoming.providers {
        if provider.id.trim().is_empty() {
            continue;
        }
        if let Some(key) = provider.api_key.as_deref() {
            let key = key.trim();
            if key.is_empty() {
                credentials.delete(&provider.id)?;
            } else {
                credentials.set(&provider.id, key)?;
            }
        }
    }
    if let Some(web) = &incoming.web_search {
        for provider in &web.providers {
            if provider.id.trim().is_empty() {
                continue;
            }
            if let Some(key) = provider.api_key.as_deref() {
                let id = search_credential_id(provider.id.trim());
                let key = key.trim();
                if key.is_empty() {
                    credentials.delete(&id)?;
                } else {
                    credentials.set(&id, key)?;
                }
            }
        }
    }

    let next = incoming.merge(load());
    write_settings(&next)?;
    Ok(next.view(credentials))
}

fn write_settings(settings: &Settings) -> Result<(), String> {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|err| format!("cannot create {}: {err}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(|err| format!("cannot serialise settings: {err}"))?;
    fs::write(&path, json).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

/// Moves API keys from an older `settings.json` into the OS credential store
/// and rewrites the file without them. If the store rejects a key it is kept in
/// memory for this session only — never written back to disk.
pub fn migrate_legacy_keys(credentials: &Credentials) {
    let mut settings = load();
    let moved = migrate_provider_keys(&mut settings, credentials);
    if moved == 0 {
        return;
    }
    match write_settings(&settings) {
        Ok(()) => eprintln!(
            "inkwell: moved {moved} API key(s) out of settings.json into the credential store"
        ),
        Err(err) => {
            eprintln!("inkwell: could not rewrite settings.json without plaintext keys: {err}")
        }
    }
}

fn migrate_provider_keys(settings: &mut Settings, credentials: &Credentials) -> usize {
    let mut moved = 0usize;
    for provider in &mut settings.providers {
        if provider.api_key.trim().is_empty() {
            continue;
        }
        let key = std::mem::take(&mut provider.api_key);
        if let Err(err) = credentials.set(&provider.id, &key) {
            eprintln!(
                "inkwell: could not import the saved key for {} into the credential store ({err}); \
                 keeping it for this session only",
                provider.label
            );
            credentials.set_session_only(&provider.id, &key);
        }
        moved += 1;
    }
    for provider in &mut settings.web_search.providers {
        if provider.api_key.trim().is_empty() {
            continue;
        }
        let key = std::mem::take(&mut provider.api_key);
        let id = search_credential_id(&provider.id);
        if let Err(err) = credentials.set(&id, &key) {
            eprintln!(
                "inkwell: could not import the saved web-search key for {} into the credential store ({err}); \
                 keeping it for this session only",
                provider.label
            );
            credentials.set_session_only(&id, &key);
        }
        moved += 1;
    }
    moved
}

/// True for OpenAI-compatible endpoints on localhost, where an API key is
/// generally not required (Ollama, LM Studio, vLLM, ...).
pub fn is_local_endpoint(base_url: &str) -> bool {
    let lower = base_url.to_ascii_lowercase();
    lower.contains("//localhost")
        || lower.contains("//127.0.0.1")
        || lower.contains("//[::1]")
        || lower.contains("//0.0.0.0")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_with(key: Option<&str>) -> SettingsInput {
        SettingsInput {
            active_provider: "openai".into(),
            providers: vec![ProviderInput {
                id: "openai".into(),
                label: "OpenAI".into(),
                kind: "openai".into(),
                base_url: " https://api.openai.com/v1/ ".into(),
                model: " gpt-4o-mini ".into(),
                api_key: key.map(str::to_string),
            }],
            temperature: 5.0,
            max_tokens: 1,
            system_prompt: "hi".into(),
            include_document: true,
            thinking: false,
            web_search: None,
        }
    }

    #[test]
    fn merge_trims_config_and_never_carries_keys() {
        let merged = input_with(None).merge(Settings::default());
        assert!(merged.providers[0].api_key.is_empty());
        assert_eq!(merged.providers[0].base_url, "https://api.openai.com/v1");
        assert_eq!(merged.providers[0].model, "gpt-4o-mini");
        assert_eq!(merged.temperature, 2.0);
        assert_eq!(merged.max_tokens, 64);
    }

    #[test]
    fn view_reads_presence_from_credentials_without_leaking_keys() {
        let credentials = Credentials::memory_only();
        credentials.set("openai", "sk-secret").unwrap();
        let view = Settings::default().view(&credentials);
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("sk-secret"));
        assert!(json.contains("\"hasKey\":true"));
        assert!(json.contains("\"backend\":\"In-memory session store\""));
        assert!(json.contains("\"available\":false"));
    }

    #[test]
    fn view_reflects_a_cleared_credential() {
        let credentials = Credentials::memory_only();
        credentials.set("openai", "sk-secret").unwrap();
        credentials.delete("openai").unwrap();
        let view = Settings::default().view(&credentials);
        let json = serde_json::to_string(&view).unwrap();
        assert!(json.contains("\"hasKey\":false"));
    }

    #[test]
    fn web_search_is_optional_and_clamps_its_configuration() {
        let mut input = input_with(None);
        input.web_search = Some(WebSearchInput {
            enabled: true,
            active_provider: "brave".into(),
            providers: vec![SearchProviderInput {
                id: "brave".into(),
                label: "Brave".into(),
                kind: "brave".into(),
                endpoint: " https://api.search.brave.com/res/v1/web/search/ ".into(),
                api_key: None,
            }],
            max_results: 99,
            timeout_secs: 1,
        });
        let merged = input.merge(Settings::default());
        assert!(merged.web_search.enabled);
        assert_eq!(merged.web_search.active_provider, "brave");
        assert_eq!(merged.web_search.max_results, 10);
        assert_eq!(merged.web_search.timeout_secs, 5);
        assert_eq!(
            merged.web_search.providers[0].endpoint,
            "https://api.search.brave.com/res/v1/web/search"
        );
        let json = serde_json::to_string(&merged).unwrap();
        assert!(!json.contains("brave-key"));
    }

    #[test]
    fn web_search_is_disabled_by_default() {
        let settings = Settings::default();
        assert!(!settings.web_search.enabled);
        assert!(settings.web_search.providers.len() >= 5);
    }

    #[test]
    fn legacy_plaintext_keys_are_migrated_and_never_serialized() {
        let credentials = Credentials::memory_only();
        let mut settings = Settings::default();
        settings.providers[0].api_key = "sk-legacy".into();

        let moved = migrate_provider_keys(&mut settings, &credentials);

        assert_eq!(moved, 1);
        assert!(settings.providers[0].api_key.is_empty());
        assert_eq!(
            credentials.get("openai").unwrap().as_deref(),
            Some("sk-legacy")
        );
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("sk-legacy"));
    }

    #[test]
    fn detects_local_endpoints() {
        assert!(is_local_endpoint("http://localhost:11434/v1"));
        assert!(is_local_endpoint("http://127.0.0.1:1234/v1"));
        assert!(!is_local_endpoint("https://api.openai.com/v1"));
    }
}
