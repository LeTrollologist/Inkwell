#!/usr/bin/env python3
"""
Inkwell local release pipeline.

Adapted from the local-first release architecture used by VPack Archiver and
Hollow-Canvas: build, test, package, verify and publish from one deterministic
command on the developer's Windows machine. No GitHub Actions required.

Stages:
  1. preflight  Tool, version and clean-tree checks
  2. build      cargo build --release --locked
  3. test       cargo test --locked + UI consistency check
  4. security   Optional cargo-audit log
  5. package    Portable zip + Inno Setup installer
  6. verify     Asset naming, archive contents and SHA-256 checksums
  7. publish    GitHub release via gh (draft supported)
"""

import argparse
import hashlib
import os
import re
import shutil
import subprocess
import sys
import time
import zipfile
from pathlib import Path

PROJECT_NAME = "inkwell"
REPO_GH = "LeTrollologist/Inkwell"
ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "dist"
APP_EXE = ROOT / "target" / "release" / "inkwell.exe"
INSTALLER_ISCC = ROOT / "installer" / "inkwell.iss"
CHANGELOG = ROOT / "CHANGELOG.md"
WEBVIEW2_LOADER = ROOT / "assets" / "runtime" / "x64" / "WebView2Loader.dll"
WEBVIEW2_LOADER_SHA256 = (
    "86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5"
)

ASSET_RE = re.compile(
    r"^(?:"
    r"inkwell-v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?-windows-x86_64\.zip"
    r"|Inkwell-Setup-\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?\.exe"
    r")$"
)

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")


def log(stage, message):
    print(f"\n[{stage.upper()}] {message}")


def fail(message):
    print(f"\nERROR: {message}", file=sys.stderr)
    sys.exit(1)


def run(cmd, check=True, capture=False, env=None):
    printable = " ".join(str(part) for part in cmd)
    print(f"  $ {printable}")
    try:
        if capture:
            result = subprocess.run(
                cmd,
                cwd=ROOT,
                env=env,
                text=True,
                encoding="utf-8",
                errors="replace",
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            if check and result.returncode != 0:
                if result.stdout:
                    print(result.stdout)
                if result.stderr:
                    print(result.stderr, file=sys.stderr)
                fail(f"command failed with exit code {result.returncode}: {printable}")
            return result
        result = subprocess.run(cmd, cwd=ROOT, env=env)
        if check and result.returncode != 0:
            fail(f"command failed with exit code {result.returncode}: {printable}")
        return result
    except FileNotFoundError:
        fail(f"required tool not found: {cmd[0]}")


def normalize_tag(value):
    value = value.strip()
    return value if value.startswith("v") else f"v{value}"


def numeric_version(version):
    base = version.split("-", 1)[0]
    parts = base.split(".")
    while len(parts) < 4:
        parts.append("0")
    return ".".join(parts[:4])


def cargo_version():
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(
        r"(?ms)^\[package\].*?^version\s*=\s*\"([^\"]+)\"", text
    )
    if not match:
        fail("could not read the version from Cargo.toml")
    return match.group(1)


def iscc_path():
    candidates = [
        os.environ.get("ISCC"),
        shutil.which("ISCC"),
        shutil.which("iscc"),
    ]
    for base in (
        os.environ.get("ProgramFiles(x86)"),
        os.environ.get("ProgramFiles"),
    ):
        if base:
            candidates.append(str(Path(base) / "Inno Setup 6" / "ISCC.exe"))
    local = os.environ.get("LOCALAPPDATA")
    if local:
        candidates.extend(
            [
                str(Path(local) / "Programs" / "Inno Setup 6" / "ISCC.exe"),
                str(Path(local) / "Programs" / "Inno" / "ISCC.exe"),
                str(Path(local) / "Inno Setup 6" / "ISCC.exe"),
            ]
        )
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate)
    return None


def mingw_env():
    env = os.environ.copy()
    mingw = ROOT / "tools" / "w64devkit" / "bin"
    if (mingw / "gcc.exe").is_file():
        env["PATH"] = str(mingw) + os.pathsep + env.get("PATH", "")
    return env


def git_clean():
    result = run(["git", "status", "--porcelain"], capture=True)
    return not result.stdout.strip()


def tag_exists(tag):
    local = run(
        ["git", "rev-parse", "--verify", f"refs/tags/{tag}"],
        check=False,
        capture=True,
    )
    if local.returncode != 0:
        fail(
            f"tag {tag} does not exist. Create and push it first:\n"
            f"  git tag -a {tag} -m \"Inkwell {tag}\"\n"
            f"  git push origin {tag}"
        )
    remote = run(["git", "ls-remote", "--tags", "origin", tag], capture=True)
    if not remote.stdout.strip():
        fail(
            f"tag {tag} is not on origin. Push it first: git push origin {tag}"
        )



def stage_preflight(publish, skip_installer, allow_dirty, version, tag):
    log("preflight", "Checking tools, version and repository state...")
    if sys.platform != "win32":
        fail("the Inkwell release pipeline currently targets Windows only")

    required = ["cargo", "rustc", "git", "gh"]
    missing = [tool for tool in required if not shutil.which(tool)]
    if missing:
        fail("missing required tools: " + ", ".join(missing))

    for tool in ("cargo", "rustc"):
        result = run([tool, "--version"], capture=True)
        print(f"  {result.stdout.strip()}")
    result = run(["gh", "--version"], capture=True)
    print(f"  {result.stdout.splitlines()[0]}")

    if publish:
        auth = run(["gh", "auth", "status"], check=False, capture=True)
        if auth.returncode != 0:
            fail("gh is not authenticated. Run: gh auth login")

    found_iscc = iscc_path()
    if skip_installer:
        print("  Installer build skipped (--skip-installer)")
    elif not found_iscc:
        fail(
            "Inno Setup not found. Install it with 'winget install JRSoftware.InnoSetup' "
            "or run with --skip-installer for a portable-only package."
        )
    else:
        print(f"  Inno Setup: {found_iscc}")

    node = shutil.which("node")
    print(f"  Node.js: {node or 'not found (UI consistency check will be skipped)'}")

    if not allow_dirty and not git_clean():
        fail(
            "the working tree is dirty. Commit the release changes first "
            "(or pass --allow-dirty for a local package-only build)."
        )

    actual = cargo_version()
    if actual != version:
        fail(
            f"Cargo.toml version is {actual}, but the release tag is {tag}. "
            "Bump Cargo.toml and CHANGELOG.md together."
        )
    print(f"  Version: {actual}")

    if publish:
        tag_exists(tag)
        print(f"  Tag: {tag} (verified locally and on origin)")


def stage_build():
    log("build", "Building the optimized release executable...")
    run(["cargo", "build", "--release", "--locked"], env=mingw_env())
    if not APP_EXE.is_file():
        fail(f"build finished but {APP_EXE} is missing")
    print(f"  {APP_EXE.name}: {APP_EXE.stat().st_size // 1024} KB")


def stage_test():
    log("test", "Running the Rust test suite and UI checks...")
    run(["cargo", "test", "--locked"], env=mingw_env())
    node = shutil.which("node")
    if node:
        run([node, "scripts/check-ui.js"])
    else:
        print("  node not found; skipped scripts/check-ui.js")


def stage_security(tag_dir):
    log("security", "Recording dependency advisory state (best effort)...")
    audit_dir = tag_dir / "audit"
    audit_dir.mkdir(parents=True, exist_ok=True)
    audit_file = audit_dir / "cargo-audit.txt"

    result = run(
        ["cargo", "audit", "--color", "never"],
        check=False,
        capture=True,
    )
    if result.returncode == 0:
        state = "cargo audit: clean"
    else:
        state = (
            "cargo audit did not pass (missing tool, vulnerabilities, or advisory "
            "database unavailable); review the log before publishing"
        )
    audit_file.write_text(
        "Inkwell release audit\n"
        f"Generated: {time.strftime('%Y-%m-%d %H:%M:%S')}\n"
        f"Status: {state}\n\n"
        f"STDOUT\n{result.stdout}\n\nSTDERR\n{result.stderr}\n",
        encoding="utf-8",
    )
    print(f"  {audit_file.relative_to(ROOT)}")
    if result.returncode != 0:
        print(f"  Warning: {state}")


def stage_package(tag, version, tag_dir, skip_installer):
    log("package", f"Packaging Inkwell {tag}...")
    staging = tag_dir / f"inkwell-v{version}-windows-x86_64"
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True, exist_ok=True)

    files = [
        (APP_EXE, "inkwell.exe"),
        (WEBVIEW2_LOADER, "WebView2Loader.dll"),
        (ROOT / "README.md", "README.md"),
        (ROOT / "INSTALL.md", "INSTALL.md"),
        (ROOT / "LICENSE", "LICENSE"),
        (ROOT / "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_NOTICES.md"),
        (ROOT / "CHANGELOG.md", "CHANGELOG.md"),
        (ROOT / "assets" / "icons" / "icon.ico", "inkwell.ico"),
    ]
    for source, name in files:
        if not source.is_file():
            fail(f"missing release file: {source}")
        shutil.copy2(source, staging / name)
        print(f"  staged {name} ({source.stat().st_size // 1024} KB)")

    if sha256_file(staging / "WebView2Loader.dll") != WEBVIEW2_LOADER_SHA256:
        fail("staged WebView2Loader.dll does not match the pinned Microsoft loader")

    # Zip timestamps must be after 1980 for compatibility.
    now = time.time()
    for item in staging.rglob("*"):
        if item.is_file():
            os.utime(item, (now, now))

    zip_path = tag_dir / f"inkwell-v{version}-windows-x86_64.zip"
    if zip_path.exists():
        zip_path.unlink()
    print(f"  creating {zip_path.name}...")
    shutil.make_archive(
        str(tag_dir / f"inkwell-v{version}-windows-x86_64"),
        "zip",
        root_dir=tag_dir,
        base_dir=staging.name,
    )
    assets = [zip_path]

    if skip_installer:
        print("  installer skipped (--skip-installer); portable zip only")
    else:
        iscc = iscc_path()
        if not iscc:
            fail("Inno Setup was not found while packaging")
        installer = tag_dir / f"Inkwell-Setup-{version}.exe"
        if installer.exists():
            installer.unlink()
        print(f"  compiling {INSTALLER_ISCC.relative_to(ROOT)} with Inno Setup...")
        run(
            [
                str(iscc),
                f"/DMyAppVersion={version}",
                f"/DMyAppVersionNumeric={numeric_version(version)}",
                f"/O{tag_dir}",
                str(INSTALLER_ISCC),
            ]
        )
        if not installer.is_file():
            matches = sorted(tag_dir.glob("Inkwell-Setup-*.exe"))
            if len(matches) == 1:
                matches[0].rename(installer)
            else:
                fail(
                    "Inno Setup finished but the expected installer was not found: "
                    f"{installer}"
                )
        print(f"  installer: {installer.name} ({installer.stat().st_size // 1024} KB)")
        assets.append(installer)

    return assets


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def stage_verify(tag, version, tag_dir, assets, skip_installer):
    log("verify", "Checking asset names, archive contents and checksums...")
    hashes = {}
    lines = []

    for asset in assets:
        if not ASSET_RE.match(asset.name):
            fail(f"asset does not match the canonical naming convention: {asset.name}")
        digest = sha256_file(asset)
        hashes[asset.name] = digest
        lines.append(f"{digest}  {asset.name}")
        print(f"  {asset.name}: {digest}")

    zip_path = next((asset for asset in assets if asset.suffix == ".zip"), None)
    if not zip_path:
        fail("portable zip was not produced")
    prefix = f"inkwell-v{version}-windows-x86_64/"
    required = {
        "inkwell.exe",
        "WebView2Loader.dll",
        "inkwell.ico",
        "README.md",
        "INSTALL.md",
        "LICENSE",
        "THIRD_PARTY_NOTICES.md",
        "CHANGELOG.md",
    }
    with zipfile.ZipFile(zip_path) as archive:
        names = archive.namelist()
        if any(not name.startswith(prefix) for name in names):
            fail(f"zip entries must live under {prefix}")
        present = {Path(name).name for name in names}
        missing = required - present
        if missing:
            fail("zip is missing: " + ", ".join(sorted(missing)))
        print(f"  zip contents: {len(names)} entries, all under {prefix}")

    installer = next(
        (asset for asset in assets if asset.name.startswith("Inkwell-Setup-")), None
    )
    if installer:
        with installer.open("rb") as handle:
            if handle.read(2) != b"MZ":
                fail(f"{installer.name} is not a Windows executable")
        if installer.stat().st_size < 500 * 1024:
            fail(f"{installer.name} looks too small for a self-contained installer")
    elif not skip_installer:
        fail("installer was not produced")
    else:
        print("  installer validation skipped")

    checksums = tag_dir / "SHA256SUMS.txt"
    checksums.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"  checksums: {checksums.name}")
    return hashes



def changelog_section(version):
    text = CHANGELOG.read_text(encoding="utf-8")
    match = re.search(rf"(?m)^##\s+\[?{re.escape(version)}\]?.*$", text)
    if not match:
        return "_See CHANGELOG.md for the full history._"
    rest = text[match.end() :]
    following = re.search(r"(?m)^##\s+", rest)
    end = match.end() + (following.start() if following else len(rest))
    return text[match.start() : end].strip()


def write_release_notes(tag, version, tag_dir, assets, hashes):
    installer = next(
        (asset for asset in assets if asset.name.startswith("Inkwell-Setup-")), None
    )
    zip_path = next(asset for asset in assets if asset.suffix == ".zip")
    changelog = changelog_section(version)

    download_rows = []
    if installer:
        download_rows.append(
            f"| `{installer.name}` | **Recommended** — per-user Windows installer "
            "(Start Menu shortcut, uninstaller, optional Markdown association) |"
        )
    download_rows.append(
        f"| `{zip_path.name}` | Portable build — extract anywhere and run `inkwell.exe` |"
    )
    download_rows.append("| `SHA256SUMS.txt` | SHA-256 checksums for every asset |")

    installer_hash = hashes.get(installer.name, "not built") if installer else "not built"
    zip_hash = hashes[zip_path.name]

    verify_lines = []
    if installer:
        verify_lines.append(f"certutil -hashfile .\\{installer.name} SHA256")
    verify_lines.append(f"certutil -hashfile .\\{zip_path.name} SHA256")
    verify_lines.append("Get-Content .\\SHA256SUMS.txt")

    body = f"""## Inkwell {tag}

A calm, local-first Markdown editor and reader with optional web search and a
bring-your-own-key LLM copilot. No account, no proxy, no telemetry — documents render locally,
and model and search requests go straight to the providers you configure.

### What's new

{changelog}

### Install (recommended)

1. Download `{installer.name if installer else 'Inkwell-Setup-<version>.exe'}` below.
2. Run it. The installer is per-user and does **not** require administrator rights.
3. Launch **Inkwell** from the Start Menu and open a Markdown file.
4. If you want the copilot, open **Settings** (`Ctrl+,`) and add a provider and API key. Reading
   Markdown needs no account or key.

The optional **Open Markdown files with Inkwell** task associates `.md`, `.markdown`, `.mdx`,
`.mdown` and `.mkd`. Leave it unchecked if you prefer another editor.

**Windows SmartScreen:** this build is not code-signed. If Windows shows “Windows protected
your PC”, choose **More info → Run anyway**, or verify the checksum first.

### Downloads

| Asset | Description |
| --- | --- |
{chr(10).join(download_rows)}

### Requirements

- Windows 10 or 11, 64-bit.
- Microsoft Edge WebView2 Runtime (preinstalled on current Windows; setup checks for it).
- An API key only if you use the copilot.

### Verify

```powershell
{chr(10).join(verify_lines)}
```

Installer SHA-256: `{installer_hash}`

Portable zip SHA-256: `{zip_hash}`

### Upgrade and uninstall

Run the new installer to upgrade in place; settings, keys and shortcuts are preserved.
Uninstall from **Settings → Apps → Installed apps → Inkwell**. Uninstalling leaves your
settings and keys in place for a future reinstall; see [INSTALL.md](https://github.com/{REPO_GH}/blob/main/INSTALL.md)
for data locations and cleanup commands.

### Notes

- Windows 10/11 x64 is the validated runtime. The native installer and file associations are
  Windows-only.
- Release builds are unsigned and may trigger SmartScreen on first launch.
- Full usage, security details and limitations: [README.md](https://github.com/{REPO_GH}#readme).
- Release process: [RELEASE_PROCESS.md](https://github.com/{REPO_GH}/blob/main/RELEASE_PROCESS.md).
"""
    notes_file = tag_dir / "release_notes.md"
    notes_file.write_text(body, encoding="utf-8")
    print(f"  notes: {notes_file.relative_to(ROOT)}")
    return notes_file



def stage_publish(tag, version, tag_dir, assets, draft):
    log("publish", f"Publishing Inkwell {tag} to GitHub...")
    notes_file = tag_dir / "release_notes.md"
    checksums = tag_dir / "SHA256SUMS.txt"
    upload = [str(asset) for asset in assets] + [str(checksums)]
    title = f"Inkwell {tag}"

    existing = run(
        ["gh", "release", "view", tag, "--json", "isDraft,url"],
        check=False,
        capture=True,
    )
    if existing.returncode == 0:
        print("  release already exists; updating notes and assets...")
        run(
            [
                "gh",
                "release",
                "edit",
                tag,
                "--title",
                title,
                "--notes-file",
                str(notes_file),
            ]
        )
        run(["gh", "release", "upload", tag, *upload, "--clobber"])
    else:
        cmd = [
            "gh",
            "release",
            "create",
            tag,
            *upload,
            "--verify-tag",
            "--title",
            title,
            "--notes-file",
            str(notes_file),
        ]
        if draft:
            cmd.append("--draft")
        if "-" in version:
            cmd.append("--prerelease")
        run(cmd)

    view = run(["gh", "release", "view", tag, "--json", "url"], capture=True)
    print(f"  {view.stdout.strip()}")


def main():
    parser = argparse.ArgumentParser(
        description="Build, package and publish Inkwell releases locally."
    )
    parser.add_argument("version", help="release tag or version, e.g. v0.1.0")
    parser.add_argument(
        "--draft", action="store_true", help="create a draft GitHub release"
    )
    parser.add_argument(
        "--no-publish",
        action="store_true",
        help="build and package locally without touching GitHub",
    )
    parser.add_argument(
        "--skip-tests", action="store_true", help="skip cargo test and UI checks"
    )
    parser.add_argument(
        "--skip-installer",
        action="store_true",
        help="produce only the portable zip (no Inno Setup required)",
    )
    parser.add_argument(
        "--allow-dirty",
        action="store_true",
        help="allow packaging from a dirty working tree (never for publishing)",
    )
    args = parser.parse_args()

    tag = normalize_tag(args.version)
    version = tag[1:]
    if not re.match(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$", version):
        fail(f"invalid version {version!r}; expected MAJOR.MINOR.PATCH[-prerelease]")

    publish = not args.no_publish
    tag_dir = DIST / tag
    tag_dir.mkdir(parents=True, exist_ok=True)

    stage_preflight(
        publish=publish,
        skip_installer=args.skip_installer,
        allow_dirty=args.allow_dirty,
        version=version,
        tag=tag,
    )
    stage_build()
    if not args.skip_tests:
        stage_test()
    stage_security(tag_dir)

    assets = stage_package(
        tag, version, tag_dir, skip_installer=args.skip_installer
    )
    hashes = stage_verify(
        tag,
        version,
        tag_dir,
        assets,
        skip_installer=args.skip_installer,
    )
    write_release_notes(tag, version, tag_dir, assets, hashes)

    if publish:
        stage_publish(tag, version, tag_dir, assets, draft=args.draft)

    print("\nRelease assets:")
    for asset in assets:
        print(f"  {asset.relative_to(ROOT)}")
    print(f"  {(tag_dir / 'SHA256SUMS.txt').relative_to(ROOT)}")
    if publish:
        print(f"\nDone: Inkwell {tag}")
    else:
        print(f"\nLocal package ready in {tag_dir.relative_to(ROOT)}")


if __name__ == "__main__":
    main()

