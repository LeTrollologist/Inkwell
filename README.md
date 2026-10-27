# Inkwell

A sleek desktop Markdown viewer in Rust with a built-in, bring-your-own-key LLM copilot.

Inkwell renders Markdown locally in Rust (`pulldown-cmark`), displays it in a frameless
WebView2 window driven by `wry` + `tao`, and streams chat completions from the provider of
your choice. No account, no proxy, no telemetry — requests go from your machine straight to
the provider.

![Inkwell reading a Markdown document with the assistant open](docs/screenshot.png)

```
┌──────────────────────────────────────────────────────────────────────┐
│  M Inkwell     Field Notes — Inkwell · 2.4 KB   ▤  <>  ☾  ⚙  Ask    │
├────────────┬───────────────────────────────────┬─────────────────────┤
│  Open file │  # Reading, without the noise     │  Assistant      + × │
│  Paste     │                                   │  OpenAI · gpt-4o    │
│            │  A calm place for long Markdown.  │                     │
│  ON PAGE   │                                   │  > summarize this   │
│  Why…      │  > The best reading interface…    │                     │
│  Delights  │                                   │  Here are the key   │
│  Code      │  | Detail | Behaviour |           │  points…            │
│            │  |--------|-----------|           │                     │
│  RECENT    │                                   │  [ Document ]  Send │
│  notes.md  │  ```rust                          │                     │
└────────────┴───────────────────────────────────┴─────────────────────┘
```

## Highlights

- **Fast, local rendering** — GFM tables, task lists, footnotes, strikethrough and code
  fences, rendered by `pulldown-cmark` in Rust.
- **Reading experience** — tuned typography, outline with scroll-spy, reading-progress bar,
  source view, print/PDF export, dark & light themes, frameless window with native drag and
  resize.
- **Any model, your key** — OpenAI, Anthropic, Google Gemini, Groq, Mistral, DeepSeek,
  OpenRouter, xAI, Ollama, LM Studio, or any OpenAI-compatible endpoint.
- **Real streaming** — SSE parsing in Rust, token-by-token markdown rendering, a separate
  collapsible *Reasoning* channel for reasoning models, and a working Stop button.
- **Document-aware** — attach the open document to every message, or quote a passage and
  ask about it.
- **Keys stay put** — the webview never receives an API key; the Rust core owns them.
- **No npm, no bundler** — the UI is plain HTML/CSS/JS embedded in the executable and served
  over a private protocol.

## Requirements

- Windows 10/11 with **WebView2** (preinstalled on current Windows).
- **Rust** stable, 1.77 or newer.
- A Windows linker for your toolchain:
  - MSVC toolchain: Visual Studio Build Tools with the Windows SDK.
  - GNU toolchain: **MinGW-w64** (`x86_64-w64-mingw32-gcc`). `scripts\dev.ps1` adds a local
    `tools\w64devkit\bin` folder to `PATH` automatically when present.

  `.cargo\config.toml` pins the GNU target to GCC because the LLVM linker bundled with some
  GNU toolchains miscompiles large binaries on Windows (the app dies at startup with an access
  violation). If you switch to MSVC, the pin does not apply.

## Run

```powershell
cd inkwell
cargo run                # or: scripts\dev.ps1
```

UI files are embedded at compile time, so after editing `ui/` rebuild with `cargo run`.

## Build

```powershell
cargo build --release    # or: scripts\build.ps1
```

The executable lands at `target/release/inkwell.exe` and is self-contained (the UI and window
icon are embedded). There is no installer yet — wrap the exe with Inno Setup, WiX or a
MSIX package if you need one.

## Connect a model

1. Press <kbd>Ctrl</kbd>+<kbd>,</kbd> (or click the provider chip at the bottom right).
2. Pick a provider from the list.
3. Paste your API key. Local endpoints do not need one.
4. Press **Fetch list** to populate model names, or type one manually.
5. **Save settings** — the dot next to *Assistant* lights up when the provider is ready.

| Provider | Transport | Default base URL |
| --- | --- | --- |
| OpenAI | `openai` | `https://api.openai.com/v1` |
| Anthropic | `anthropic` | `https://api.anthropic.com` |
| Google Gemini | `gemini` | `https://generativelanguage.googleapis.com` |
| Groq | `openai` | `https://api.groq.com/openai/v1` |
| Mistral | `openai` | `https://api.mistral.ai/v1` |
| DeepSeek | `openai` | `https://api.deepseek.com/v1` |
| OpenRouter | `openai` | `https://openrouter.ai/api/v1` |
| xAI | `openai` | `https://api.x.ai/v1` |
| Ollama | `openai` | `http://localhost:11434/v1` |
| LM Studio | `openai` | `http://localhost:1234/v1` |
| Custom | `openai` | whatever you type |

"OpenAI-compatible" covers vLLM, LiteLLM, llama.cpp's server, Together, Fireworks and
anything else that speaks `/chat/completions` with SSE.

Preset model names are starting points and providers rename models often. Use **Fetch list**
to see what your key can actually reach, then pick from the datalist.

### Try it without a key

A mock OpenAI-compatible server ships with the repo so the UI can be exercised offline:

```powershell
node scripts\mock-llm-server.js
```

In Settings choose **Custom endpoint**, set the base URL to `http://127.0.0.1:8787/v1`,
model `mock-thinker`, and leave the API key empty. You get streamed Markdown, a reasoning
channel and a fake model list — all local.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+O` | Open a Markdown file |
| `Ctrl+J` | Toggle the assistant |
| `Ctrl+B` | Toggle the sidebar |
| `Ctrl+U` | Rendered ⇄ source |
| `Ctrl+,` | Assistant settings |
| `Ctrl+P` | Print / save as PDF |
| `Enter` | Send (Shift+Enter for a newline) |
| `Ctrl+Enter` | Send from anywhere |
| `Esc` | Close dialogs |

Drag a `.md` file onto the window, or use **Paste** to render Markdown from the clipboard.
Starting Inkwell with a file argument (`inkwell.exe notes.md`) opens it directly.

## Security & privacy

- **Local rendering.** Markdown is parsed in Rust; raw HTML inside a document is escaped
  rather than injected, so a document or model answer cannot run script in the webview.
- **Keys never reach the page.** The frontend only sees `hasKey` booleans. Keys live in
  `%APPDATA%\Inkwell\settings.json` (path shown in Settings) and are only used by the Rust
  HTTP client.
- **Locked-down webview.** A strict CSP allows only embedded scripts/styles and blocks
  script/XHR network access (`connect-src 'none'`); provider traffic happens in Rust. Remote
  images referenced by a document still load like any browser image (and reveal your IP to
  that host).
- **External links** are handed to the OS browser; only `http(s)`/`mailto` are accepted.
- **Your data.** The open document is included in a request only when *Document* is
  enabled, and only to the provider you configured.

Keys are stored as plain JSON in your user profile — the usual trade-off for a local-first
BYOK app. If you need OS-level encryption at rest, wrap that file with your platform's
secret store (DPAPI/Keychain/libsecret) before shipping.


## Architecture

```
inkwell/
├── Cargo.toml
├── src/
│   ├── main.rs        tao event loop, frameless window, embedded webview
│   ├── app.rs         IPC dispatch, async bridge, custom asset protocol
│   ├── llm.rs         OpenAI / Anthropic / Gemini SSE streaming, model listing
│   ├── markdown.rs    pulldown-cmark renderer, outline/anchors, HTML escaping
│   ├── settings.rs    persisted settings, key masking, preset providers
│   └── assets.rs      include_bytes! asset map served to the webview
├── ui/
│   ├── index.html     shell markup
│   ├── styles.css     design system, reading typography, print styles
│   ├── app.js         state, document UX, chat streaming
│   ├── bridge.js      invoke/listen over wry's postMessage IPC
│   └── vendor/        highlight.js + theme (bundled for offline use)
├── assets/icons/      source PNG/ICO icons
├── docs/              demo document + screenshot
└── scripts/           dev/build/test, mock provider, icon generator, UI check
```

The window is frameless; dragging and edge resizing are forwarded from the titlebar and
invisible edge strips to native `drag_window` / `drag_resize_window` calls. All document
content is served from memory over `http://inkwell.localhost`, so nothing depends on the
working directory.

Frontend → Rust commands (JSON over `window.ipc.postMessage`):

| Command | Purpose |
| --- | --- |
| `render_markdown` | Markdown → HTML + outline + word count |
| `get_settings` / `save_settings` | Masked settings view / merge + persist |
| `list_models` | Provider model discovery |
| `chat` | Spawn a streaming completion (`llm:delta/done/error` events) |
| `cancel_chat` | Cooperative cancellation for one stream |
| `open_external` | Open a link in the system browser |
| `initial_document` | File passed on the command line |
| `app_info` | Version and settings path |
| `win_*` | Minimize / maximize / close / drag / resize |

Events are request-scoped by a generated id, so overlapping streams can never mix.

## Tests

```powershell
scripts\test.ps1         # or: cargo test
node scripts\check-ui.js # duplicate functions / missing ids / markup balance
```

Covers anchor-id generation and deduplication, front-matter extraction, escaping of raw
HTML, GFM tables/task lists, SSE parsing for all three transports, request construction and
key masking.

## Notes & limitations

- Inkwell is a **viewer**: it renders Markdown but does not edit or save files.
- Remote images are fetched by the webview; relative image paths are not resolved because
  the file picker does not expose a directory.
- LaTeX is shown as written (no math renderer yet).
- The executable's file icon is not embedded in the PE (the window and taskbar icon are);
  add a resource step if you package with an installer.
- On Linux, `reqwest` uses native TLS, so `libssl-dev`/`pkg-config` are needed for builds.

## License

MIT — see [LICENSE](LICENSE).

