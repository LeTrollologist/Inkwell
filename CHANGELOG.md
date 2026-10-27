# Changelog

All notable changes to Inkwell are documented here. The pipeline reads the section
matching the release version when it builds GitHub release notes.

## [0.1.0] - 2026-10-27

First public release.

### Added

- **Windows installer** — per-user Inno Setup wizard, no administrator rights required.
  Creates Start Menu (and optional desktop) shortcuts, registers an uninstaller, and can
  associate `.md`, `.markdown`, `.mdx`, `.mdown` and `.mkd` files with Inkwell.
- **Portable build** — `inkwell-v0.1.0-windows-x86_64.zip` with the executable, docs and icon.
- **Markdown reader** — GitHub-flavoured Markdown rendered in Rust: tables, task lists,
  footnotes, strikethrough, code highlighting, outline/scroll-spy, source view, print/PDF,
  dark and light themes, frameless window with native drag and resize.
- **BYOK copilot** — OpenAI, Anthropic, Google Gemini, Groq, Mistral, DeepSeek, OpenRouter,
  xAI, Ollama, LM Studio and any OpenAI-compatible endpoint. Document context, passage
  quoting and a separate reasoning channel are supported.
- **Local document handling** — native file picker and drag/drop with real filesystem paths;
  relative images are embedded from the document folder with containment and size limits.
- **Security controls** — raw HTML is escaped, link/image schemes are allow-listed, and
  malformed or model-generated Markdown goes through the same hardened renderer.
- **Streaming robustness** — Stop cancels stalled HTTP requests; rate limits surface
  `Retry-After`; interrupted, empty and 5xx streams produce clear errors.
- **OS credential storage** — API keys live in Windows Credential Manager, macOS Keychain or
  Linux Secret Service. `settings.json` contains no secrets. A session-only, clearly labelled
  fallback is used when no credential store is available.
- **Local release pipeline** — deterministic Python pipeline adapted from VPack Archiver and
  Hollow-Canvas: preflight, build, tests, audit log, packaging, checksums and GitHub release.

### Notes

- Windows builds are currently unsigned. Windows SmartScreen may warn on first launch;
  SHA-256 checksums are published with every release.
- Windows 10/11 x64 is the validated runtime. The native installer is Windows-only; other
  platforms can build from source and use the browser file-input fallback.

## [Unreleased]

- Code signing and VirusTotal submission hooks for future releases.
- Installer branding and upgrade polish.
