# Changelog

All notable changes to Inkwell are documented here. The pipeline reads the section
matching the release version when it builds GitHub release notes.

## [1.0.0-rc] - 2026-11-09

Release candidate for 1.0, focused on editing and platform parity.

### Added

- **Split editor with live preview (#15).** A third view mode shows the Markdown source and the rendered document side by side. The preview re-renders as you type (debounced, race-safe), and the outline, reading progress, word count, unsaved marker and relative images follow the buffer. Toggle it from the new title-bar button or `Ctrl+Shift+U`; `Ctrl+U` keeps the full-window source editor.
- **Editing that stays out of the way.** `Tab` indents by two spaces and `Shift+Tab` outdents the current line instead of moving focus, the editor remembers unsaved buffers when you switch views, and editing continues to work with no provider configured.
- **Native file dialogs on macOS and Linux (#17).** The open and save dialogs now use `osascript` on macOS and `zenity`/`kdialog` on Linux, returning real filesystem paths so relative images and Save-as behave like the Win32 dialogs. The in-window file picker remains the fallback when no helper is installed.
- **Consistent drag-and-drop.** The window now highlights the drop target while a native drag is over it, explains when a dragged file is not Markdown instead of silently ignoring it, and says when extra documents were ignored. Paths come from the OS on every platform.

### Changed

- README documents the three editor views, the new shortcuts and per-platform dialog support.
- `scripts/check-ui.js` still guards the shell; the release pipeline is unchanged.

## [0.7.0] - 2026-11-09

### Added

- **Provider capability reporting in the Co-Pilot (#20).** Settings now shows what the selected provider and model actually support — streaming, model discovery, reasoning and native web-search tool calls — and recomputes the summary as you edit the model or base URL. The rules live in Rust (`settings::ProviderCapabilities`) and every provider in the settings view carries its capabilities, so the UI and the request builder share one definition.
- **Unsupported options are no longer offered blindly.** **Fetch list** is disabled when the provider has no base URL to ask, and the reasoning toggle is disabled with an inline reason when the chosen model ignores reasoning controls.
- **Clear feedback when a saved preference cannot apply.** If reasoning is enabled but the active provider and model cannot use it, the answer starts with a short notice explaining that reasoning was left out (shown once per provider and model).
- New `provider_capabilities` IPC command, so the settings form can preview capabilities for unsaved edits without persisting them.

### Changed

- `is_reasoning_model` now lives in `settings`, so the capability metadata and the OpenAI request builder cannot drift apart.

## [0.6.0] - 2026-11-08

### Added

- **API Route provider preset.** Added API Route using the existing OpenAI-compatible transport, with `https://global.api-route.com/v1` and `deepseek-v4.1-flash` as defaults. The preset carries no key and leaves the active provider unchanged; a regression test covers its endpoint, transport, model and empty key.

### Fixed

- **Relative images in documents opened without a folder path.** Documents opened through the browser file-input fallback (or restored without a saved path) previously showed a broken image for every relative reference, with no way to fix it. The renderer now reports the relative image references it could not embed, and the reader shows a notice with **Choose folder…**. The folder is read locally in the page, only files inside it that match the reported references (and pass the existing 8 MB per-image / 24 MB total / raster-only limits) are displayed, and references that climb out of the folder are never followed. References that still cannot be found are listed, so a genuine broken link is visible instead of silent. Documents opened through the native picker or drag-and-drop keep embedding images in Rust exactly as before.

## [0.5.0] - 2026-11-03

### Added

- **CheaperInference provider preset.** Added CheaperInference using the existing OpenAI-compatible transport, with `https://api.cheaperinference.com/v1` and `gpt-5.4-mini` as defaults. Fresh installs and reset settings receive the preset; existing saved provider lists remain unchanged.

### Changed

- Updated the README's third-party API privacy disclosure to avoid maintaining a provider-specific list of gateways.

## [0.4.0] - 2026-11-01

### Added

- **Demon Route provider preset.** The provider list now includes Demon Route
  (`https://api.demonroute.com/v1`, `glm-5.3-flash`) using the existing OpenAI transport.
  The README documents that it is a third-party gateway and that documents, prompts and API
  keys are handled under its own terms. The preset test verifies unique default provider ids
  and the expected transport, base URL and model. Fresh installs and reset settings receive
  the preset; existing saved provider lists are intentionally unchanged.

### Changed

- **MSVC toolchain support.** The project scripts now detect the active Rust host and import
  the Visual Studio `vcvars64` environment automatically for MSVC builds, while retaining
  MinGW-w64 support for GNU builds. The README documents both paths and the Windows SDK
  requirement.
- Release validation now runs with the MSVC toolchain on this machine: all **73 tests** pass,
  and the optimized MSVC executable launches successfully.

## [0.3.3] - 2026-10-31

### Improved

- **Edit mode now requires a complete, closed fenced Markdown document before offering
  Apply.** Unfenced prose, partial responses and explanations no longer produce an editable
  proposal. When the contract is missed, the chat shows a clear failure card with a
  **Retry edit** action instead of applying the wrong content.
- The edit parser is much more tolerant of real model output: three or four backticks, tildes,
  mixed-length closing fences, optional language tags, surrounding commentary and nested code
  fences are handled. This fixes the Gemini-style responses that previously failed.
- Edit proposals now include a collapsible Markdown preview, line/word counts, a copy action,
  and an **Undo** action after applying. Applied proposals remain available after reloading the
  app.
- The edit prompt now requests exactly one three-backtick closed block and forbids commentary
  outside it, while the parser still protects against model variations.

## [0.3.2] - 2026-10-31

### Fixed

- **Edit mode accepts full-document responses without a code fence.** Models sometimes return
  the revised Markdown directly, or use tilde/unclosed fences. Inkwell now treats any non-empty
  edit response as a proposed document, shows the review card, and warns when no fenced block
  was detected instead of failing with "did not return a complete fenced document".
- The edit system prompt now explicitly allows a plain full-document response when fences are
  unavailable.

### Removed

- Removed the broken v0.1.0 GitHub release and tag, plus its changelog entry. Use v0.1.1 or later.

## [0.3.1] - 2026-10-31

### Fixed

- **API keys no longer disappear when Web search is incomplete.** If Web search was enabled
  but its provider key or endpoint was missing, Save used to stop before persisting any chat
  provider changes. The chat key was therefore never stored, and sending a prompt reopened
  Settings with an empty key field. Inkwell now saves the chat provider normally, closes
  Settings, and shows a non-blocking warning that Web search still needs configuration.
- Web search remains unavailable until its own provider and key are complete; it no longer
  blocks saving the rest of the assistant settings.

## [0.3.0] - 2026-10-31

### Added

- **Agentic web search** — an optional Web search assistant mode. The model can call a
  `web_search` tool when it needs current information, then answer with the retrieved excerpts
  and numbered source links. Supports Tavily, Brave Search, Serper, Exa and self-hosted SearXNG
  with bring-your-own-key settings.
- Web search is disabled by default and configured under *Settings → Web search*. Search keys
  are stored in the OS credential store under a separate namespace and never reach the webview.
- Search activity and results stream into the chat; source links open in the system browser.
- Search requests have hard timeouts, cancellation through the existing Stop button, typed
  authentication/rate-limit/timeout errors, and URL/HTML sanitisation. External excerpts are
  explicitly marked as untrusted context in the model prompt.
- Native web-search tool-call planning for OpenAI-compatible, Anthropic and Gemini transports,
  with a JSON planning fallback for models that do not support tools.

### Changed

- Renamed the existing local **Search doc** mode to **Fetch doc**. The underlying
  heading-aware document retrieval is unchanged; the IPC command is now `fetch_document`
  (`search_document` remains as a compatibility alias).
- Chat composer modes are now Ask / Fetch doc / Web search / Edit doc. The remembered mode
  migrates from the old `search` value automatically.
- Select controls now follow the active dark or light theme, including their dropdown options.

## [0.2.0] - 2026-10-28

### Added

- **Markdown editor** — the source pane is now an editor with `Ctrl+S`, `Ctrl+Shift+S`
  (save as), unsaved-change indicators, confirmation before discarding edits, and a native
  save dialog for untitled documents.
- **Search mode** — local heading-aware retrieval over the open document. Retrieved passages
  are included in the prompt, the assistant is asked to cite headings, and the UI lists
  clickable sources under the answer.
- **Edit mode** — the assistant proposes a complete revised Markdown document in a fenced
  block; a review card lets you apply it to the editor without saving automatically.
- **Reasoning/thinking toggle** — opt-in provider controls for OpenAI reasoning models
  (`max_completion_tokens` + `reasoning_effort`), Anthropic extended thinking
  (`thinking.budget_tokens`), Gemini `thinkingConfig`, and OpenRouter `reasoning_effort`.
- **LaTeX math** — `$…$` and `$$…$$` are parsed by `pulldown-cmark` and rendered offline with
  bundled KaTeX 0.16.22 and fonts.
- **PE resources** — application icon and version metadata are embedded in `inkwell.exe`, so
  Explorer, shortcuts and Add/Remove Programs show the proper Inkwell icon.

### Changed

- Chat composer has Ask / Search doc / Edit doc modes; the mode is remembered locally.
- Document edits update the outline, preview and session while preserving unsaved state.
- README documents the editor, assistant modes, reasoning support and math.

## [0.1.1] - 2026-10-27

### Fixed

- **Missing WebView2 loader.** The GNU Windows build links `WebView2Loader.dll`
  dynamically, but v0.1.0's installer and portable ZIP did not include it. On machines
  without a copy on `PATH`, `inkwell.exe` failed at startup with a Windows application-error
  dialog. The loader is now bundled next to `inkwell.exe` in both the installer and the ZIP.
- `build.rs` stages `WebView2Loader.dll` in `target/debug` / `target/release`, so local
  `cargo run` no longer depends on a loader that happens to be on the developer's PATH.
- The installer now ships `THIRD_PARTY_NOTICES.md` for the Microsoft loader.

### Notes

- The pinned loader is Microsoft WebView2Loader `1.0.3800.47` (x64), SHA-256
  `86545B66CDB0603BC26B626FB9AD610CB6E71F28D468F5EA66DF23B03DDA96D5`.
- If you installed v0.1.0, uninstall it and install v0.1.1 (or later). Settings and API keys
  are preserved.

## [Unreleased]

### Added

- **`--smoke-test`.** Opens the window, loads the UI and waits up to 30 seconds for the frontend to answer `app_ready` over IPC, then exits 0 (1 on timeout). It is the runtime check that works without an interactive desktop, so CI and packagers can verify a build actually renders.
- **CI workflow** (`.github/workflows/ci.yml`): builds and tests on Windows, macOS and Ubuntu, runs the UI checks, and runs the smoke test under Xvfb/D-Bus on Linux.
- **`docs/PLATFORMS.md`**: build requirements, validation checklist, known limitations and the per-platform status table.

### Fixed

- **Blank window on macOS and Linux.** wry only intercepts `<scheme>://` custom-protocol URLs there (Windows uses `http://<scheme>.localhost`), and Inkwell always loaded the Windows form, so the webview requested a real HTTP page. The entry URL is now chosen per platform.
- **Linux on Wayland.** The webview is created through GTK (`build_gtk`); wry's plain `build` only supports X11 on Linux.

### Planned

- Record a macOS and a Linux build/runtime result in `docs/PLATFORMS.md` (issue #16) once CI can run or real hardware is available.
- Code signing and VirusTotal submission hooks for future releases.
- Installer branding and upgrade polish.
