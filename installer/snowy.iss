; Snowy installer - Inno Setup 6.
; Compile with: ISCC.exe /DMyAppVersion=0.1.0 installer\snowy.iss
#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif
#define MyAppName "Snowy"
#define MyAppExe "snowy.exe"

[Setup]
AppId={{B7D3C1E4-5A0F-4E86-9C2D-53A1F0E7A9B2}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher=Snowy
LicenseFile=..\LICENSE
InfoBeforeFile=privacy.txt
AppCopyright=Copyright (c) 2026 berkcantmr
DefaultDirName={localappdata}\Programs\Snowy
DefaultGroupName=Snowy
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=Snowy-{#MyAppVersion}-setup
SetupIconFile=..\assets\snowy.ico
UninstallDisplayIcon={app}\{#MyAppExe}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
; In-app updates run this installer with /SILENT /CLOSEAPPLICATIONS: running Snowy processes are
; closed, and the [Run] entry below relaunches Snowy ourselves.
CloseApplications=yes
RestartApplications=no
VersionInfoVersion={#MyAppVersion}
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppVersion}
VersionInfoCompany=Snowy
VersionInfoDescription=Snowy setup
VersionInfoCopyright=Copyright (c) 2026 berkcantmr

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
; Simplified Chinese ships with Snowy itself but not with Inno Setup 6, so the installer
; skips it.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Snowy"; Filename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\Snowy"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon

[Registry]
; Remove the autostart entry the app may have created.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "Snowy"; Flags: uninsdeletevalue dontcreatekey

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "{cm:LaunchProgram,Snowy}"; Flags: nowait postinstall skipifsilent
; Silent (in-app) updates relaunch Snowy.
Filename: "{app}\{#MyAppExe}"; Flags: nowait; Check: WizardSilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/IM {#MyAppExe} /F"; Flags: runhidden; RunOnceId: "KillSnowy"

[Code]
// Make sure no Snowy process holds snowy.exe before files are replaced (in-app updates start
// this installer while the tray app is still running). First ask nicely (WM_CLOSE: Snowy saves
// and quits), then force whatever is left. Runs before Setup's own files-in-use check.
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  RC: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM {#MyAppExe}', '', SW_HIDE, ewWaitUntilTerminated, RC);
  if RC = 0 then
    Sleep(1500);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM {#MyAppExe} /F', '', SW_HIDE, ewWaitUntilTerminated, RC);
  if RC = 0 then
    Sleep(500);
  Result := '';
end;
