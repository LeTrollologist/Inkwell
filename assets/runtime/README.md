# WebView2 Loader

`x64/WebView2Loader.dll` is the Microsoft Edge WebView2 loader required by the
GNU Windows build. On non-MSVC targets `webview2-com-sys` links this DLL
dynamically, so it must sit next to `inkwell.exe` at runtime.

- File version: `1.0.3800.47`
- Source: `webview2-com-sys` 0.39.1 (crates.io), `x64/WebView2Loader.dll`
- SHA-256: `86545B66CDB0603BC26B626FB9AD610CB6E71F28D468F5EA66DF23B03DDA96D5`
- Copyright Microsoft Corporation. Distributed under the Microsoft Edge WebView2
  SDK license terms; see `THIRD_PARTY_NOTICES.md`.

`build.rs` copies this file into `target/debug` or `target/release` so local
`cargo run` works. The release pipeline copies it into both the portable ZIP and
the installer.
