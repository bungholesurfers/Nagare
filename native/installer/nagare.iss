; Nagare's Windows installer, for Inno Setup 6 (https://jrsoftware.org/isinfo.php). installer.ps1, beside this folder,
; builds the release exes and compiles this into target\installer\nagare-setup-VERSION.exe.
;
; It installs for the user alone, with no administrator: into %LOCALAPPDATA%\Programs\Nagare, where Windows keeps
; per-user programs (VS Code's user installer and others go there), with a Start menu shortcut and an entry in
; Settings > Apps whose InstallLocation is what tells the app it is installed (src/autostart.rs). Starting with Windows
; is a task, ticked to begin with; the app's panel can switch it later. The config, in %APPDATA%\Nagare, is the user's
; and is left on uninstall.
;
; How it looks - its style, colours, pictures and icon - is theme\theme.iss, which nagare-customize's Installer tab
; writes, with the pictures beside it. INSTALLER.md, beside this file, says how to build it.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Bin
  #define Bin "..\target\release"
#endif

[Setup]
; The id names the entry in Settings > Apps across versions: never change it.
AppId={{5A4B7ADD-FD41-4B23-B650-E1A7212B29A9}
AppName=Nagare
AppVersion={#AppVersion}
AppVerName=Nagare {#AppVersion}
VersionInfoVersion={#AppVersion}
PrivilegesRequired=lowest
; {autopf} is %LOCALAPPDATA%\Programs for a per-user install.
DefaultDirName={autopf}\Nagare
; The welcome page shows the theme's big picture, as the finish page does.
DisableWelcomePage=no
DisableProgramGroupPage=yes
DisableDirPage=auto
DisableReadyPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
UninstallDisplayIcon={app}\nagare.exe
UninstallDisplayName=Nagare
OutputDir=..\target\installer
OutputBaseFilename=nagare-setup-{#AppVersion}
Compression=lzma2/max
SolidCompression=yes
; The app and its watchdog are stopped by the code below, not by the Restart Manager, which would leave the
; watchdog to start the app again.
CloseApplications=no
#include "theme\theme.iss"

[Tasks]
Name: "autostart"; Description: "Start Nagare when I sign in"
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#Bin}\nagare.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Bin}\nagare-watchdog.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Nagare"; Filename: "{app}\nagare.exe"
Name: "{autodesktop}\Nagare"; Filename: "{app}\nagare.exe"; Tasks: desktopicon

[Registry]
; The same value the panel's Start with Windows writes; the uninstaller removes it whichever wrote it (see below).
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Nagare"; \
  ValueData: """{app}\nagare.exe"""; Flags: uninsdeletevalue; Tasks: autostart
; The app's name and picture for its notifications, which the app writes as it first shows one (tray.rs).
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\Nagare"; Flags: uninsdeletekey dontcreatekey

[UninstallDelete]
; What Software Update downloads (update.rs): the last installer it fetched, and its log.
Type: filesandordirs; Name: "{%TEMP}\nagare-update"

[Run]
Filename: "{app}\nagare.exe"; Description: "Start Nagare now"; Flags: nowait postinstall skipifsilent

[Code]
var
  WasRunning: Boolean;

// Stops a running Nagare: the watchdog first, which would otherwise start the app again, then the app. Notes whether
// the app was running (taskkill ends 0 when it stopped something), so that a silent update can start it again.
procedure StopNagare();
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/f /im nagare-watchdog.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
  if Exec(ExpandConstant('{sys}\taskkill.exe'), '/f /im nagare.exe', '', SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0) then
    WasRunning := True;
  Sleep(300);
end;

// A silent install skips the finish page's Start Nagare now: a Nagare it stopped is started again, the new one.
procedure CurStepChanged(CurStep: TSetupStep);
var
  Code: Integer;
begin
  if (CurStep = ssPostInstall) and WizardSilent and WasRunning then
    ExecAsOriginalUser(ExpandConstant('{app}\nagare.exe'), '', '', SW_SHOWNORMAL, ewNoWait, Code);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopNagare();
  Result := '';
end;

function InitializeUninstall(): Boolean;
begin
  StopNagare();
  Result := True;
end;

// The panel can turn starting with Windows on after the install, and Task Manager can mark it off: both go with the app.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Nagare');
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run', 'Nagare');
  end;
end;
