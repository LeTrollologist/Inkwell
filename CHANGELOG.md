# Changelog

All notable changes to Inkwell are documented here. The pipeline reads the section
matching the release version when it builds GitHub release notes.

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

- Code signing and VirusTotal submission hooks for future releases.
- Installer branding and upgrade polish.
