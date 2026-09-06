; Lecoo Control Center Windows x64 installer.
; Build with scripts\windows\package.ps1 so the version, source directory,
; and output directory are supplied consistently for local and CI releases.

#ifndef AppVersion
  #define AppVersion "0.5.2-beta"
#endif

#ifndef SourceDir
  #define SourceDir "..\..\target\x86_64-pc-windows-msvc\release"
#endif

#ifndef ProjectRoot
  #define ProjectRoot "..\.."
#endif

#ifndef OutputDir
  #define OutputDir "..\..\dist"
#endif

#define AppName "Lecoo Control Center"
#define AppPublisher "LaVashikk"
#define AppUrl "https://github.com/LaVashikk/Lecoo-Control-Center"
#define ServiceName "LecooControlDaemon"
#define ServiceDisplayName "Lecoo EC Daemon"
#define InstallDirectory "LecooControlCenter"

[Setup]
AppId={{6BE7D849-3757-4613-9C50-B8E7C6E2FA83}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}/issues
AppUpdatesURL={#AppUrl}/releases
DefaultDirName={autopf}\{#InstallDirectory}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
LicenseFile={#ProjectRoot}\LICENSE
OutputDir={#OutputDir}
OutputBaseFilename=Lecoo-Control-Center-{#AppVersion}-Windows-x64-Setup
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesEnvironment=yes
CloseApplications=yes
RestartApplications=no
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\lecoo-control-center.exe

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "chinesesimp"; MessagesFile: "{#ProjectRoot}\scripts\windows\languages\ChineseSimplified.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\lecoo-ec-daemon.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\lecoo-ctrl.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\lecoo-control-center.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ProjectRoot}\libs\inpoutx64.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ProjectRoot}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ProjectRoot}\README.md"; DestDir: "{app}"; DestName: "README.md"; Flags: ignoreversion
Source: "{#ProjectRoot}\README_CN.md"; DestDir: "{app}"; DestName: "README_CN.md"; Flags: ignoreversion; AfterInstall: FinalizeInstall

[Icons]
Name: "{autoprograms}\{#AppName}\{#AppName}"; Filename: "{app}\lecoo-control-center.exe"; WorkingDir: "{app}"
Name: "{autoprograms}\{#AppName}\命令行帮助"; Filename: "{cmd}"; Parameters: "/K ""{app}\lecoo-ctrl.exe"" help"; WorkingDir: "{app}"
Name: "{autoprograms}\{#AppName}\卸载 {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\lecoo-control-center.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\lecoo-control-center.exe"; Description: "启动 {#AppName}"; WorkingDir: "{app}"; Flags: nowait postinstall skipifsilent

[Code]
const
  ServiceName = '{#ServiceName}';
  EnvironmentKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  StartupValue = 'LecooControlCenter';

function RunSc(const Parameters: String; var ResultCode: Integer): Boolean;
begin
  Result := Exec(ExpandConstant('{sys}\sc.exe'), Parameters, '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

function ServiceExists: Boolean;
var
  ResultCode: Integer;
begin
  Result := RunSc('query "' + ServiceName + '"', ResultCode) and (ResultCode = 0);
end;

function ServiceHasState(const State: String): Boolean;
var
  ResultCode: Integer;
  Command: String;
begin
  Command := '/C ""' + ExpandConstant('{sys}\sc.exe') + '" query "' + ServiceName +
    '" | "' + ExpandConstant('{sys}\find.exe') + '" /I "' + State + '" >nul"';
  Result := Exec(ExpandConstant('{cmd}'), Command, '', SW_HIDE, ewWaitUntilTerminated, ResultCode) and
    (ResultCode = 0);
end;

procedure StopAndRemoveExistingService(const Required: Boolean);
var
  ResultCode: Integer;
  Attempt: Integer;
begin
  if ServiceExists then begin
    Log('Stopping existing ' + ServiceName + ' service.');
    RunSc('stop "' + ServiceName + '"', ResultCode);
    for Attempt := 1 to 15 do begin
      if ServiceHasState('STOPPED') then begin
        Break;
      end;
      Sleep(1000);
    end;

    if not ServiceHasState('STOPPED') then begin
      Log('Service did not stop in time; terminating a lingering daemon process.');
      Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM lecoo-ec-daemon.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
      Sleep(1000);
    end;

    RunSc('delete "' + ServiceName + '"', ResultCode);
    if ResultCode <> 0 then begin
      if Required then begin
        RaiseException('Unable to remove the previous Lecoo service. Restart Windows and run Setup again.');
      end;
      Exit;
    end;

    for Attempt := 1 to 15 do begin
      if not ServiceExists then begin
        Break;
      end;
      Sleep(1000);
    end;

    if ServiceExists and Required then begin
      RaiseException('The previous Lecoo service is still pending deletion. Restart Windows and run Setup again.');
    end;
  end;

  Log('Ensuring no lingering Lecoo daemon process locks the release files.');
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM lecoo-ec-daemon.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Sleep(1000);
end;

procedure InstallAndStartService;
var
  ResultCode: Integer;
  Attempt: Integer;
  DaemonPath: String;
  CreateParameters: String;
begin
  DaemonPath := ExpandConstant('{app}\lecoo-ec-daemon.exe');
  CreateParameters := 'create "' + ServiceName + '" binPath= "\"' + DaemonPath +
    '\" --service" start= auto depend= RpcSs DisplayName= "{#ServiceDisplayName}"';

  if (not RunSc(CreateParameters, ResultCode)) or (ResultCode <> 0) then begin
    RaiseException('Unable to register the Lecoo Windows service (sc create failed).');
  end;

  RunSc('description "' + ServiceName + '" "Lecoo laptop EC hardware control daemon"', ResultCode);
  RunSc('failure "' + ServiceName + '" reset= 86400 actions= restart/5000/restart/5000/""/0', ResultCode);

  if (not RunSc('start "' + ServiceName + '"', ResultCode)) or (ResultCode <> 0) then begin
    RaiseException('The Lecoo Windows service could not be started. See the Setup log for details.');
  end;

  for Attempt := 1 to 15 do begin
    Sleep(1000);
    // The daemon spends roughly two seconds preparing the service before it
    // touches the EC. Waiting three seconds prevents Setup from declaring
    // success for a process that immediately exits during hardware startup.
    if (Attempt >= 3) and ServiceHasState('RUNNING') then begin
      Exit;
    end;
  end;

  RaiseException('The Lecoo Windows service did not reach the running state. See the Setup log for details.');
end;

function NormalizeDirectory(const Value: String): String;
begin
  Result := Trim(Value);
  while (Length(Result) > 3) and (Result[Length(Result)] = '\') do begin
    Delete(Result, Length(Result), 1);
  end;
end;

function IsInstallDirectory(const Value: String): Boolean;
begin
  Result := CompareText(NormalizeDirectory(Value), NormalizeDirectory(ExpandConstant('{app}'))) = 0;
end;

procedure SplitSemicolonSeparated(const Value: String; var Entries: TArrayOfString);
var
  StartIndex: Integer;
  EndIndex: Integer;
  EntryCount: Integer;
begin
  SetArrayLength(Entries, 0);
  StartIndex := 1;
  while StartIndex <= Length(Value) + 1 do begin
    EndIndex := StartIndex;
    while (EndIndex <= Length(Value)) and (Value[EndIndex] <> ';') do begin
      EndIndex := EndIndex + 1;
    end;
    EntryCount := GetArrayLength(Entries);
    SetArrayLength(Entries, EntryCount + 1);
    Entries[EntryCount] := Copy(Value, StartIndex, EndIndex - StartIndex);
    StartIndex := EndIndex + 1;
  end;
end;

procedure AddInstallDirectoryToMachinePath;
var
  ExistingPath: String;
  Entries: TArrayOfString;
  Index: Integer;
begin
  if not RegQueryStringValue(HKLM, EnvironmentKey, 'Path', ExistingPath) then begin
    ExistingPath := '';
  end;

  SplitSemicolonSeparated(ExistingPath, Entries);
  for Index := 0 to GetArrayLength(Entries) - 1 do begin
    if IsInstallDirectory(Entries[Index]) then begin
      Exit;
    end;
  end;

  if ExistingPath <> '' then begin
    ExistingPath := ExistingPath + ';';
  end;
  RegWriteExpandStringValue(HKLM, EnvironmentKey, 'Path', ExistingPath + ExpandConstant('{app}'));
end;

procedure RemoveInstallDirectoryFromMachinePath;
var
  ExistingPath: String;
  Entries: TArrayOfString;
  Index: Integer;
  NewPath: String;
begin
  if not RegQueryStringValue(HKLM, EnvironmentKey, 'Path', ExistingPath) then begin
    Exit;
  end;

  SplitSemicolonSeparated(ExistingPath, Entries);
  NewPath := '';
  for Index := 0 to GetArrayLength(Entries) - 1 do begin
    if (Entries[Index] <> '') and (not IsInstallDirectory(Entries[Index])) then begin
      if NewPath <> '' then begin
        NewPath := NewPath + ';';
      end;
      NewPath := NewPath + Entries[Index];
    end;
  end;

  if NewPath <> ExistingPath then begin
    RegWriteExpandStringValue(HKLM, EnvironmentKey, 'Path', NewPath);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopAndRemoveExistingService(True);
  NeedsRestart := False;
  Result := '';
end;

procedure FinalizeInstall;
begin
  Log('Registering and starting the Lecoo Windows service.');
  InstallAndStartService;
  AddInstallDirectoryToMachinePath;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then begin
    StopAndRemoveExistingService(False);
    RemoveInstallDirectoryFromMachinePath;
    RegDeleteValue(HKCU, RunKey, StartupValue);
  end;
end;
