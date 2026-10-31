# Third-party notices

## Microsoft Edge WebView2 Loader

`assets/runtime/x64/WebView2Loader.dll`

- Copyright Microsoft Corporation. All rights reserved.
- File version: 1.0.3800.47
- Obtained from `webview2-com-sys` 0.39.1 (`x64/WebView2Loader.dll`, crates.io).
- Used to load the Microsoft Edge WebView2 Runtime from the native `wry` shell.
- Distributed as part of the Microsoft Edge WebView2 SDK and subject to its
  license terms: https://learn.microsoft.com/microsoft-edge/webview2/
- SHA-256: `86545B66CDB0603BC26B626FB9AD610CB6E71F28D468F5EA66DF23B03DDA96D5`

The rest of Inkwell is MIT licensed; see `LICENSE`. Bundled JavaScript libraries
under `ui/vendor/` retain their own license headers.

## KaTeX

`ui/vendor/katex/katex.min.js`, `katex.min.css`, and `fonts/*.woff2`

- Copyright Khan Academy and KaTeX contributors.
- Version: 0.16.22.
- MIT license; full text in `ui/vendor/katex/LICENSE`.
- Used for local LaTeX math rendering; no math content is sent to a third party.
