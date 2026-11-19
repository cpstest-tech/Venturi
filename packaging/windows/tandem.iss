; Built by scripts/build-windows.sh, which defines AppVersion, SourceDir and
; OutputDir.

#define AppName "Tandem"

[Setup]
AppId={{6F1C2E4A-7B3D-4E8F-9A21-5C0D8B7E3F46}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Tandem
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
; Per-user by default, so no admin rights are needed; the dialog offers both.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesAssociations=yes
LicenseFile={#SourceDir}\LICENSE.txt
SetupIconFile=..\..\media\icons\windows\tandem.ico
UninstallDisplayIcon={app}\tandem.exe
OutputDir={#OutputDir}
OutputBaseFilename=Tandem-windows-x64-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "it"; MessagesFile: "compiler:Languages\Italian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\tandem.exe"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\tandem.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\.tandem"; ValueType: string; ValueName: ""; ValueData: "Tandem.Project"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Tandem.Project"; ValueType: string; ValueName: ""; ValueData: "Tandem project"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Tandem.Project\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\tandem.exe,0"
Root: HKA; Subkey: "Software\Classes\Tandem.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\tandem.exe"" ""%1"""

[Run]
Filename: "{app}\tandem.exe"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
