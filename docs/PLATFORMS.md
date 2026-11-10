# Platform support

Inkwell ships Windows builds. The same source is expected to build on macOS and Linux, and this
page records exactly what is validated today, what each platform needs, and what is still
missing. Contributions with real hardware are welcome — see [Validating a platform](#validating-a-platform).

## Status

| | Windows 10/11 x64 | macOS 12+ | Linux (X11 / Wayland) |
| --- | --- | --- | --- |
| Build | Validated — every release is built here | Not yet run | Not yet run |
| Automated tests (`cargo test`) | Validated locally (87 tests) | Not yet run | Not yet run |
| Runtime: window, webview, IPC | Validated on Windows 10/11 | Not yet run | Not yet run |
| Installer | Inno Setup, validated | Not applicable (build from source) | Not applicable (build from source) |

"Inkwell ships Windows builds" is a statement of fact, not a claim that the other platforms are
broken: the source is platform-neutral apart from small `cfg` blocks, and the runtime differences
that were found have been fixed (see [Fixed for 1.0](#fixed-for-10)). They simply have not been
launched on real hardware by the maintainers yet.

## Build requirements

Common: Rust 1.88 or newer and Git. The UI has no npm/bundler step.

### Windows 10/11

- MSVC Build Tools (`Microsoft.VisualStudio.Component.VC.Tools.x86.x64`) or MinGW-w64.
- Microsoft Edge WebView2 Runtime (preinstalled on current Windows).
- Inno Setup 6.3+ only when building the installer.

### macOS

- Xcode Command Line Tools (`xcode-select --install`) for the system frameworks.
- No extra runtime dependency: WebKit is part of macOS.

```sh
cargo build --release --locked
./target/release/inkwell --smoke-test
```

### Linux

- GTK 3 and WebKitGTK 4.1 development packages, plus TLS headers for `reqwest`:

  ```sh
  # Debian / Ubuntu
  sudo apt-get install -y build-essential pkg-config libssl-dev \
      libgtk-3-dev libwebkit2gtk-4.1-dev
  ```

- Runtime only: a Secret Service provider (GNOME Keyring, KWallet) for persistent API keys, and
  `zenity` or `kdialog` for native file dialogs. Without them Inkwell still runs: keys become
  session-only (Settings shows a warning) and the in-window file picker is used.
- X11 or Wayland. The webview is created through GTK, which covers both.

```sh
cargo build --release --locked
./target/release/inkwell --smoke-test
```

## Fixed for 1.0

Two platform assumptions were found while preparing this page and are fixed in the source:

- **Entry URL scheme.** wry only intercepts custom-protocol URLs of the form
  `<scheme>://<host>` on macOS and Linux; Windows (and Android) use the
  `http://<scheme>.localhost` workaround. Inkwell hardcoded the Windows form, which meant the
  webview requested a real HTTP page on macOS and Linux and showed a blank window. The URL is now
  chosen per platform (`inkwell://localhost/index.html` off Windows).
- **Linux webview creation.** wry's `WebViewBuilder::build(&window)` supports X11 only on Linux.
  The Linux path now builds the webview through GTK (`build_gtk`), so Wayland sessions work too.

## Known limitations

| Platform | Limitation |
| --- | --- |
| macOS | The custom frameless window has no working edge-resize strips: `tao` reports `drag_resize_window` as `NotSupported` on macOS. Use the title-bar maximize button or the system window menu. |
| macOS | Release builds are unsigned and not notarized; Gatekeeper will require right-click → Open the first time. |
| macOS | Native dialogs are driven through `osascript` (the system Open/Save panels). No installer is produced. |
| Linux | Dragging the frameless window by its title bar uses `tao`'s `drag_window`, which is X11-only; on Wayland move the window with the compositor shortcut (usually Alt+F7). |
| Linux | Persistent API keys need a running Secret Service; otherwise Inkwell keeps keys in memory for the session and says so in Settings. |
| Linux | Native dialogs need `zenity` or `kdialog`; otherwise the in-window file picker is used. |
| Linux | `reqwest` uses native TLS, so a build needs OpenSSL headers (`libssl-dev`). |
| All non-Windows | The installer, file associations and PE icon/version metadata are Windows-only. |

## Validating a platform

`--smoke-test` launches the real binary, loads the UI in the platform webview and waits for the
frontend to report `app_ready` over IPC, exiting 0 on success and 1 after 30 seconds. It needs a
display (on a headless Linux box: `xvfb-run -a ./target/release/inkwell --smoke-test`).

For a full manual pass:

1. **Launch** — the window opens with the empty state (no blank webview).
2. **IPC** — `Ctrl+,` opens Settings with values loaded; the status bar shows *Ready*.
3. **Files** — `Ctrl+O` opens the native dialog; drag a `.md` onto the window; pass a path on the
   command line. The document keeps its filesystem path.
4. **Rendering** — GFM tables, code highlighting, KaTeX math and a relative image next to the
   document all render.
5. **Editing** — `Ctrl+U` (source) and `Ctrl+Shift+U` (split) edit, preview live and save with
   `Ctrl+S`.
6. **Credentials** — save a provider key; it should be readable after a restart. Deny the keychain
   prompt (or stop Secret Service) and Inkwell must fall back to session-only keys with a visible
   warning instead of failing.
7. **Quit** — closing the window exits the process.

## Local validation

There is no hosted CI, by design: the release pipeline is local
([RELEASE_PROCESS.md](../RELEASE_PROCESS.md)). Every pipeline run now includes a `runtime` stage
that starts the built executable with `--smoke-test` and writes the outcome to
`dist/<tag>/runtime-report.md` — OS, architecture, Rust host, and whether the webview loaded and
answered IPC.

To record a result for a platform, run this on that platform:

```sh
python3 scripts/pipeline.py 1.0.0-rc --validate-only
```

`--validate-only` builds, runs the test suite and the smoke test, writes the report, and stops —
nothing is packaged or published. Unlike the release path (Windows-only because of the installer),
it runs on macOS and Linux. On a headless machine use `--skip-runtime`, which records an explicit
skip instead of a result.

The Windows numbers above come from this pipeline on Windows. The macOS and Linux rows stay
"not yet run" until someone runs the command on one of those machines and commits the resulting
report next to this page.
