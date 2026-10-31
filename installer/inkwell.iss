; Inkwell per-user Windows installer.
; Build with scripts/pipeline.py (it passes MyAppVersion and MyAppVersionNumeric).
; Requires Inno Setup 6.3 or newer.

#define MyAppName "Inkwell"
#define MyAppPublisher "LeTrollologist"
#define MyAppURL "https://github.com/LeTrollologist/Inkwell"
#define MyAppExeName "inkwell.exe"

#ifndef MyAppVersion
  #define MyAppVersion "0.3.2"
#endif
#ifndef MyAppVersionNumeric
  #define MyAppVersionNumeric "0.3.2.0"
#endif

[Setup]
AppId={{DA252FB3-C33B-4A74-9422-4182ACF821E7}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases
DefaultDirName={localappdata}\Programs\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE
PrivilegesRequired=lowest
OutputDir=..\dist\installer
OutputBaseFilename=Inkwell-Setup-{#MyAppVersion}
SetupIconFile=..\assets\icons\icon.ico
UninstallDisplayIcon={app}\inkwell.ico
UninstallDisplayName={#MyAppName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
VersionInfoVersion={#MyAppVersionNumeric}
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription={#MyAppName} installer
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppVersionNumeric}
CloseApplications=yes
RestartApplications=no
UsePreviousAppDir=yes
UsePreviousTasks=yes
ChangesAssociations=yes
SetupLogging=yes
AllowNoIcons=yes

[Tasks]
Name: "associate"; Description: "Open Markdown files with Inkwell (.md, .markdown, .mdx)"; GroupDescription: "File associations:"; Flags: checkedonce
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\inkwell.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\runtime\x64\WebView2Loader.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\icons\icon.ico"; DestDir: "{app}"; DestName: "inkwell.ico"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\INSTALL.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Inkwell"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\inkwell.ico"
Name: "{group}\{cm:UninstallProgram,Inkwell}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Inkwell"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\inkwell.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,Inkwell}"; Flags: nowait postinstall skipifsilent

[Registry]
; Optional .md/.markdown/.mdx association. Removed on uninstall.
Root: HKCU; Subkey: "Software\Classes\.md"; ValueType: string; ValueName: ""; ValueData: "Inkwell.Markdown"; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\.markdown"; ValueType: string; ValueName: ""; ValueData: "Inkwell.Markdown"; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\.mdx"; ValueType: string; ValueName: ""; ValueData: "Inkwell.Markdown"; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\.mdown"; ValueType: string; ValueName: ""; ValueData: "Inkwell.Markdown"; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\.mkd"; ValueType: string; ValueName: ""; ValueData: "Inkwell.Markdown"; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\Inkwell.Markdown"; ValueType: string; ValueName: ""; ValueData: "Markdown document"; Flags: uninsdeletekey; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\Inkwell.Markdown\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\inkwell.ico"; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\Inkwell.Markdown\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\Applications\inkwell.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Inkwell"; Tasks: associate
Root: HKCU; Subkey: "Software\Classes\Applications\inkwell.exe\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Flags: uninsdeletekey; Tasks: associate

[Code]
function WebView2Present(): Boolean;
begin
  { The Evergreen runtime lives in Program Files (x86) or, for per-user
    installs, under LOCALAPPDATA. }
  Result := DirExists(ExpandConstant('{pf32}\Microsoft\EdgeWebView\Application')) or
            DirExists(ExpandConstant('{localappdata}\Microsoft\EdgeWebView\Application'));
end;

function InitializeSetup(): Boolean;
begin
  Result := True;
  if not WebView2Present() then
  begin
    if not WizardSilent() then
      MsgBox('Inkwell needs the Microsoft Edge WebView2 Runtime, which is not installed.' + Chr(13) + Chr(10) +
             Chr(13) + Chr(10) +
             'Install it from:' + Chr(13) + Chr(10) +
             'https://go.microsoft.com/fwlink/p/?LinkId=2124703' + Chr(13) + Chr(10) +
             Chr(13) + Chr(10) +
             'Then run Inkwell Setup again.', mbError, MB_OK);
    Result := False;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Value: String;
begin
  { Only remove an extension association if it still points at Inkwell.
    If the user chose another editor after installing, leave it alone. }
  if CurUninstallStep = usPostUninstall then
  begin
    if RegQueryStringValue(HKCU, 'Software\Classes\.md', '', Value) and
       (CompareText(Value, 'Inkwell.Markdown') = 0) then
      RegDeleteValue(HKCU, 'Software\Classes\.md', '');
    if RegQueryStringValue(HKCU, 'Software\Classes\.markdown', '', Value) and
       (CompareText(Value, 'Inkwell.Markdown') = 0) then
      RegDeleteValue(HKCU, 'Software\Classes\.markdown', '');
    if RegQueryStringValue(HKCU, 'Software\Classes\.mdx', '', Value) and
       (CompareText(Value, 'Inkwell.Markdown') = 0) then
      RegDeleteValue(HKCU, 'Software\Classes\.mdx', '');
    if RegQueryStringValue(HKCU, 'Software\Classes\.mdown', '', Value) and
       (CompareText(Value, 'Inkwell.Markdown') = 0) then
      RegDeleteValue(HKCU, 'Software\Classes\.mdown', '');
    if RegQueryStringValue(HKCU, 'Software\Classes\.mkd', '', Value) and
       (CompareText(Value, 'Inkwell.Markdown') = 0) then
      RegDeleteValue(HKCU, 'Software\Classes\.mkd', '');
  end;
end;

