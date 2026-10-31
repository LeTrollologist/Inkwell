# Inkwell: Local Release & Packaging Process

A deterministic, local-first release pipeline adapted from **VPack Archiver** and
**Hollow-Canvas**. Everything runs on the developer's Windows machine through
`scripts/pipeline.py` and the `gh` CLI — no GitHub Actions or external build services.

## 1. Principles

1. **Local-first** — build, test, package and publish from one command.
2. **One canonical installer** — `Inkwell-Setup-<version>.exe`, per-user, no admin prompt.
3. **Portable fallback** — `inkwell-v<version>-windows-x86_64.zip` plus SHA-256 checksums.
4. **Version discipline** — the tag, `Cargo.toml` version and `CHANGELOG.md` section agree;
   the pipeline refuses to build otherwise.
5. **Clean releases** — only the installer, portable zip and `SHA256SUMS.txt` are published.
6. **No secrets in artifacts** — API keys live in the OS credential store, never in builds.
7. **Runtime completeness** — every release bundles Microsoft `WebView2Loader.dll`; the
   pipeline pins and verifies its SHA-256 before packaging.

## 2. Prerequisites

| Tool | Purpose |
| --- | --- |
| Rust 1.88+ (`rustc`, `cargo`) | Build the application |
| MinGW-w64 (GNU target) or MSVC Build Tools | Link the Windows executable |
| Python 3.10+ | Run `scripts/pipeline.py` |
| [`gh`](https://cli.github.com/) authenticated | Publish releases |
| [Inno Setup 6.3+](https://jrsoftware.org/isinfo.php) (`ISCC.exe`) | Build the installer |
| Node.js (optional) | `scripts/check-ui.js` markup/JS checks |
| `cargo-audit` (optional) | Dependency advisory log |

Install Inno Setup with either:

```powershell
winget install JRSoftware.InnoSetup
# or
choco install innosetup -y
```

If `ISCC.exe` is not on `PATH`, the pipeline also looks in
`%ProgramFiles(x86)%\Inno Setup 6`, `%ProgramFiles%\Inno Setup 6` and
`%LOCALAPPDATA%\Programs\Inno Setup 6`.

## 3. Versioning

1. Update `version` in `Cargo.toml` to the release version.
2. Add a matching `## [<version>] - YYYY-MM-DD` section to `CHANGELOG.md`.
3. Commit the changes and merge them to `main`.
4. Create and push an **annotated** tag:

```powershell
git tag -a v0.3.0 -m "Inkwell v0.3.0"
git push origin v0.3.0
```

The pipeline checks that `Cargo.toml` says `0.3.0` and that the working tree is clean.
It never creates or moves a tag; `--verify-tag` prevents publishing against a missing tag.

## 4. Pipeline stages

| Stage | Operation |
| --- | --- |
| `preflight` | Checks Rust, `gh`, Inno Setup, Python and the clean working tree |
| `build` | `cargo build --release --locked` (uses project-local MinGW when present) |
| `test` | `cargo test --locked` plus `node scripts/check-ui.js` |
| `security` | Runs `cargo audit` when available and writes `dist/<tag>/audit/cargo-audit.txt` |
| `package` | Stages files, builds the portable zip and compiles the Inno Setup installer |
| `verify` | Validates asset names/contents and writes `SHA256SUMS.txt` |
| `publish` | Writes release notes and creates/updates the GitHub release |

## 5. Commands

```powershell
# Full release (build, test, package, publish)
python scripts\pipeline.py v0.3.0

# Draft release for review
python scripts\pipeline.py v0.3.0 --draft

# Build and package locally only (no GitHub upload)
python scripts\pipeline.py v0.3.0 --no-publish

# Portable zip only (temporary, no Inno Setup required)
python scripts\pipeline.py v0.3.0 --no-publish --skip-installer

# Skip tests only when iterating on packaging (never for a real release)
python scripts\pipeline.py v0.3.0 --no-publish --skip-tests --allow-dirty
```

Equivalent `make` targets (when GNU Make is installed):

```powershell
make draft TAG=v0.3.0
make release TAG=v0.3.0
make package TAG=v0.3.0
```

## 6. Output layout

```text
dist/v0.3.0/
├── inkwell-v0.3.0-windows-x86_64/
│   ├── inkwell.exe
│   ├── WebView2Loader.dll
│   ├── inkwell.ico
│   ├── README.md
│   ├── INSTALL.md
│   ├── LICENSE
│   ├── THIRD_PARTY_NOTICES.md
│   └── CHANGELOG.md
├── inkwell-v0.3.0-windows-x86_64.zip
├── Inkwell-Setup-0.3.0.exe
├── SHA256SUMS.txt
├── release_notes.md
└── audit/
    └── cargo-audit.txt
```

Only the zip, installer and `SHA256SUMS.txt` are uploaded. `release_notes.md` becomes the
release body; the audit log stays local unless you choose to attach it.

## 7. Verifying an installer locally

```powershell
$tmp = Join-Path $env:TEMP "inkwell-install-test"
$setup = ".\dist\v0.3.0\Inkwell-Setup-0.3.0.exe"
Start-Process $setup -ArgumentList "/VERYSILENT","/SUPPRESSMSGBOXES","/NORESTART","/DIR=$tmp" -Wait
Test-Path "$tmp\inkwell.exe"          # must be True
Test-Path "$tmp\WebView2Loader.dll"   # must be True
Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*" |
  Where-Object DisplayName -eq "Inkwell"
& "$tmp\inkwell.exe"                  # launch smoke test
& "$tmp\unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES
Test-Path "$tmp\inkwell.exe"          # must be False after uninstall
```

Test file associations with a throwaway user profile or check only the registry paths;
uninstalling removes the extension values and the `Inkwell.Markdown` ProgID.

## 8. Publishing

The pipeline uses `gh release create --verify-tag`. If the release already exists it edits the
notes and re-uploads assets with `--clobber`. To publish a draft after review:

```powershell
gh release edit v0.3.0 --draft=false
```

Release assets:

| Asset | Description |
| --- | --- |
| `Inkwell-Setup-<version>.exe` | Per-user installer (recommended) |
| `inkwell-v<version>-windows-x86_64.zip` | Portable build |
| `SHA256SUMS.txt` | SHA-256 digests for both |

## 9. Signing and SmartScreen

Release builds are currently **unsigned**, so Windows SmartScreen may show a warning. The
release notes and `INSTALL.md` explain how to verify SHA-256 and continue. Future versions can
sign `inkwell.exe` and the installer with `signtool` before packaging; the pipeline is the
single place to add that step. Do not publish a build with a personal/self-signed certificate
as if it were trusted.

## 10. Rollback

- Delete a bad release asset and re-upload a replacement:
  `gh release upload v0.3.0 <asset> --clobber`
- Mark a release as a draft while fixing it: `gh release edit v0.3.0 --draft`
- Delete a release (and optionally its tag): `gh release delete v0.3.0 --yes [--cleanup-tag]`

Never move a published tag; publish a new patch version instead.

