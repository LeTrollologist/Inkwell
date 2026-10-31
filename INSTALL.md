# Installing Inkwell

Inkwell is a local-first Markdown editor and reader with an optional bring-your-own-key LLM
copilot. The Windows installer is per-user and does **not** require administrator rights.

> **Upgrading from v0.1.0?** v0.1.0 was missing `WebView2Loader.dll` and could fail at startup
> on some machines. Install v0.1.1 or newer; your settings and keys are preserved.

## Requirements

- Windows 10 or 11, 64-bit.
- Microsoft Edge **WebView2 Runtime**. It is preinstalled on current Windows and ships with
  Microsoft Edge; the installer checks for it and links to the runtime if it is missing.

## Install (recommended)

1. Open the [latest release](https://github.com/LeTrollologist/Inkwell/releases/latest).
2. Download `Inkwell-Setup-<version>.exe`.
3. Run it and follow the wizard.
   - The default install folder is `%LOCALAPPDATA%\Programs\Inkwell`.
   - Leave **Open Markdown files with Inkwell** checked to associate `.md`, `.markdown`,
     `.mdx`, `.mdown` and `.mkd` files. This can be changed later.
   - The desktop shortcut is optional; a Start Menu shortcut is always created.
4. Launch Inkwell from the Start Menu. Add a provider and key in **Settings** (`Ctrl+,`) only
   if you want to use the copilot. Reading Markdown needs no account or key.

### SmartScreen

Release builds are currently **not code-signed**. Windows may show
â€œWindows protected your PCâ€. To continue, choose **More info â†’ Run anyway**. If you prefer,
verify the download first:

```powershell
certutil -hashfile .\Inkwell-Setup-0.3.1.exe SHA256
Get-Content .\SHA256SUMS.txt
```

The hashes must match the installer and portable zip in the release.

## Silent install / upgrade

```powershell
# Install (or upgrade in place) without prompts
.\Inkwell-Setup-0.3.1.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
```

Upgrades keep the previous install folder, shortcuts, file-association choice and all user
data. Close Inkwell before upgrading; the installer can also detect and close it.

## Portable build

Download `inkwell-v<version>-windows-x86_64.zip` and extract it anywhere. Run
`inkwell.exe`. Portable builds have no shortcuts, uninstaller or file associations; settings
and keys still use the normal per-user locations below.

## Uninstall

- **Settings â†’ Apps â†’ Installed apps â†’ Inkwell â†’ Uninstall**, or
- Run `Uninstall.exe` in the install folder.

Uninstalling removes the application, shortcuts, file associations and installer registry
entries. It intentionally leaves your settings, chat history and API keys in place so a
reinstall keeps your configuration. To remove those too:

```powershell
Remove-Item -Recurse -Force "$env:APPDATA\Inkwell"
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\Inkwell"
```

API keys are held in Windows Credential Manager under the service name
`app.inkwell.viewer`; remove them with the Windows Credential Manager UI if desired.

## Where Inkwell stores data

| Data | Location |
| --- | --- |
| Non-secret settings | `%APPDATA%\Inkwell\settings.json` |
| API keys | Windows Credential Manager (`app.inkwell.viewer`) |
| WebView2 profile, recents, theme, chat | `%LOCALAPPDATA%\Inkwell\WebView2` |
| Application files | `%LOCALAPPDATA%\Programs\Inkwell` |

## Troubleshooting

- **â€œWebView2 Runtime is not installed.â€** Install the
  [Evergreen Runtime](https://go.microsoft.com/fwlink/p/?LinkId=2124703), then run setup again.
- **Inkwell does not start.** Confirm Windows 10/11 x64 and WebView2. Try the portable build
  to see whether the issue is installation-specific.
- **SmartScreen blocks the installer.** See the verification steps above; the build is unsigned.
- **Markdown files open in another editor.** Re-run the installer and enable the file-association
  task, then use *Open with* if Windows still prefers another app.
- **A key is missing after switching machines.** Keys are stored per Windows user and are not
  included in `settings.json`; re-enter them in Settings.

For build-from-source and release instructions, see the [README](README.md) and
[RELEASE_PROCESS.md](RELEASE_PROCESS.md).
