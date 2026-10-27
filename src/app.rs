//! Native shell: window, embedded webview, IPC dispatch and streaming bridge.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tao::event_loop::EventLoopProxy;
use tao::window::{ResizeDirection, Window};
use wry::http::{Request, Response};

use crate::assets;
use crate::credentials::Credentials;
use crate::llm::{self, ChatRequest};
use crate::markdown;
use crate::settings;

pub enum UserEvent {
    Ipc(String),
    Script(String),
    Close,
}

/// Sends JavaScript into the webview (always from the event-loop thread).
#[derive(Clone)]
pub struct Emitter {
    proxy: EventLoopProxy<UserEvent>,
}

impl Emitter {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        Self { proxy }
    }

    pub fn script(&self, js: String) {
        let _ = self.proxy.send_event(UserEvent::Script(js));
    }

    pub fn event<T: Serialize>(&self, name: &str, payload: T) {
        let Ok(payload) = serde_json::to_string(&payload) else {
            return;
        };
        let Ok(name) = serde_json::to_string(name) else {
            return;
        };
        self.script(format!("window.__inkwell_event({name}, {payload});"));
    }

    pub fn reply(&self, id: &str, result: Result<Value, String>) {
        let payload = match result {
            Ok(value) => json!({ "id": id, "ok": true, "result": value }),
            Err(error) => json!({ "id": id, "ok": false, "error": error }),
        };
        if let Ok(serialised) = serde_json::to_string(&payload) {
            self.script(format!("window.__inkwell_reply({serialised});"));
        }
    }

    pub fn close(&self) {
        let _ = self.proxy.send_event(UserEvent::Close);
    }
}

pub struct AppContext {
    pub runtime: tokio::runtime::Runtime,
    pub client: reqwest::Client,
    pub chats: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub initial_doc: Mutex<Option<PathBuf>>,
    pub credentials: Arc<Credentials>,
}

impl AppContext {
    pub fn new(initial_doc: Option<PathBuf>) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to start the async runtime");
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(120))
            .user_agent(concat!("Inkwell/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build the HTTP client");
        let credentials = Arc::new(Credentials::new());
        settings::migrate_legacy_keys(&credentials);
        Self {
            runtime,
            client,
            chats: Mutex::new(HashMap::new()),
            initial_doc: Mutex::new(initial_doc),
            credentials,
        }
    }
}

#[derive(Deserialize)]
struct IpcRequest {
    id: String,
    cmd: String,
    #[serde(default)]
    args: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenedDocument {
    name: String,
    content: String,
    path: Option<String>,
    size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    config_dir: String,
    settings_file: String,
}

pub fn serve_asset(request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    match assets::lookup(request.uri().path()) {
        Some(asset) => Response::builder()
            .status(200)
            .header("content-type", asset.mime)
            .header("cache-control", "no-store")
            .body(Cow::Borrowed(asset.bytes))
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(500)
                    .body(Cow::Borrowed(&b"Internal error"[..]))
                    .expect("valid fallback response")
            }),
        None => Response::builder()
            .status(404)
            .header("content-type", "text/plain; charset=utf-8")
            .body(Cow::Borrowed(&b"Not found"[..]))
            .expect("valid not-found response"),
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "md" | "markdown" | "mdx" | "mdown" | "mkd" | "txt"
            )
        })
        .unwrap_or(false)
}

pub fn initial_path_from_args() -> Option<PathBuf> {
    std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .find(|path| path.is_file() && is_markdown(path))
}

fn read_document(path: &Path) -> Result<OpenedDocument, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|err| format!("Cannot read {}: {err}", path.display()))?;
    if metadata.len() > 16 * 1024 * 1024 {
        return Err("That file is larger than 16 MB.".into());
    }
    let bytes =
        std::fs::read(path).map_err(|err| format!("Cannot read {}: {err}", path.display()))?;
    Ok(OpenedDocument {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document.md")
            .to_string(),
        content: String::from_utf8_lossy(&bytes).to_string(),
        path: Some(path.display().to_string()),
        size: metadata.len(),
    })
}

fn open_external(url: &str) -> Result<(), String> {
    let allowed = ["http://", "https://", "mailto:"];
    if !allowed.iter().any(|prefix| url.starts_with(prefix)) {
        return Err("Only http(s) and mailto links can be opened.".into());
    }

    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();

    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();

    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();

    result
        .map(|_| ())
        .map_err(|err| format!("Could not open the link: {err}"))
}

fn resize_direction(name: &str) -> Option<ResizeDirection> {
    Some(match name {
        "n" => ResizeDirection::North,
        "s" => ResizeDirection::South,
        "e" => ResizeDirection::East,
        "w" => ResizeDirection::West,
        "ne" => ResizeDirection::NorthEast,
        "nw" => ResizeDirection::NorthWest,
        "se" => ResizeDirection::SouthEast,
        "sw" => ResizeDirection::SouthWest,
        _ => return None,
    })
}


pub fn handle_ipc(body: &str, window: &Window, context: &Arc<AppContext>, emitter: &Emitter) {
    let request: IpcRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(err) => {
            eprintln!("inkwell: invalid IPC payload: {err}");
            return;
        }
    };
    if let Some(result) = dispatch(&request, window, context, emitter) {
        emitter.reply(&request.id, result);
    }
}

fn dispatch(
    request: &IpcRequest,
    window: &Window,
    context: &Arc<AppContext>,
    emitter: &Emitter,
) -> Option<Result<Value, String>> {
    let args = &request.args;
    match request.cmd.as_str() {
        "win_minimize" => {
            window.set_minimized(true);
            Some(Ok(Value::Null))
        }
        "win_toggle_maximize" => {
            let next = !window.is_maximized();
            window.set_maximized(next);
            Some(Ok(json!(next)))
        }
        "win_is_maximized" => Some(Ok(json!(window.is_maximized()))),
        "win_close" => {
            emitter.close();
            Some(Ok(Value::Null))
        }
        "win_drag" => {
            if let Err(err) = window.drag_window() {
                eprintln!("inkwell: window drag failed: {err}");
            }
            Some(Ok(Value::Null))
        }
        "win_resize" => {
            if let Some(direction) = args
                .get("direction")
                .and_then(Value::as_str)
                .and_then(resize_direction)
            {
                let _ = window.drag_resize_window(direction);
            }
            Some(Ok(Value::Null))
        }
        "render_markdown" => {
            let source = args
                .get("markdown")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Some(serde_json::to_value(markdown::render(source)).map_err(|err| err.to_string()))
        }
        "get_settings" => Some(
            serde_json::to_value(settings::load().view(&context.credentials))
                .map_err(|err| err.to_string()),
        ),
        "save_settings" => {
            let payload = args.get("payload").cloned().unwrap_or(Value::Null);
            let payload = match serde_json::from_value::<settings::SettingsInput>(payload) {
                Ok(payload) => payload,
                Err(err) => return Some(Err(format!("invalid settings: {err}"))),
            };
            Some(
                settings::save(&context.credentials, payload)
                    .and_then(|view| serde_json::to_value(view).map_err(|err| err.to_string())),
            )
        }
        "app_info" => {
            let path = settings::settings_path();
            let info = AppInfo {
                version: env!("CARGO_PKG_VERSION").to_string(),
                config_dir: path
                    .parent()
                    .map(|dir| dir.display().to_string())
                    .unwrap_or_default(),
                settings_file: path.display().to_string(),
            };
            Some(serde_json::to_value(info).map_err(|err| err.to_string()))
        }
        "initial_document" => {
            let path = context
                .initial_doc
                .lock()
                .ok()
                .and_then(|mut guard| guard.take());
            Some(match path {
                Some(path) => read_document(&path)
                    .and_then(|doc| serde_json::to_value(doc).map_err(|err| err.to_string())),
                None => Ok(Value::Null),
            })
        }
        "open_external" => {
            let url = args.get("url").and_then(Value::as_str).unwrap_or_default();
            Some(open_external(url).map(|_| Value::Null))
        }
        "cancel_chat" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or_default();
            let mut cancelled = false;
            if let Ok(chats) = context.chats.lock() {
                if let Some(flag) = chats.get(id) {
                    flag.store(true, Ordering::Relaxed);
                    cancelled = true;
                }
            }
            Some(Ok(json!(cancelled)))
        }
        "list_models" => {
            let provider_id = args
                .get("providerId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let settings = settings::load();
            let Some(provider) = settings.provider(&provider_id).cloned() else {
                return Some(Err(format!("Unknown provider: {provider_id}")));
            };
            let provider = match resolve_provider(provider, &context.credentials) {
                Ok(provider) => provider,
                Err(err) => return Some(Err(err)),
            };
            let id = request.id.clone();
            let emitter = emitter.clone();
            let client = context.client.clone();
            let handle = context.runtime.handle().clone();
            handle.spawn(async move {
                let result = llm::fetch_models(&client, &provider)
                    .await
                    .map(|models| json!(models));
                emitter.reply(&id, result);
            });
            None
        }
        "chat" => {
            let payload = args.get("req").cloned().unwrap_or(Value::Null);
            let chat_request: ChatRequest = match serde_json::from_value(payload) {
                Ok(request) => request,
                Err(err) => return Some(Err(format!("invalid chat request: {err}"))),
            };
            let settings = settings::load();
            let Some(provider) = settings.provider(&settings.active_provider).cloned() else {
                return Some(Err(
                    "No provider is selected. Open Settings (Ctrl+,).".to_string()
                ));
            };
            let provider = match resolve_provider(provider, &context.credentials) {
                Ok(provider) => provider,
                Err(err) => return Some(Err(err)),
            };

            let cancel = Arc::new(AtomicBool::new(false));
            let id = chat_request.id.clone();
            if let Ok(mut chats) = context.chats.lock() {
                chats.insert(id.clone(), Arc::clone(&cancel));
            }

            let emitter = emitter.clone();
            let client = context.client.clone();
            let context = Arc::clone(context);
            let handle = context.runtime.handle().clone();
            handle.spawn(async move {
                llm::stream_chat(emitter, client, provider, chat_request, cancel).await;
                if let Ok(mut chats) = context.chats.lock() {
                    chats.remove(&id);
                }
            });
            Some(Ok(Value::Null))
        }
        other => Some(Err(format!("Unknown command: {other}"))),
    }
}



/// `settings.json` never carries secrets, so always resolve the key from the
/// OS credential store right before building a provider request.
fn resolve_provider(
    mut provider: settings::ProviderConfig,
    credentials: &Credentials,
) -> Result<settings::ProviderConfig, String> {
    provider.api_key = credentials.get(&provider.id)?.unwrap_or_default();
    Ok(provider)
}
