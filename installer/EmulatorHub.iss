#define MyAppName "Emulator Hub"
#define MyAppPublisher "Emulator Hub"

#ifndef MyAppVersion
  #define MyAppVersion "0.0.4.1"
#endif

#ifndef SourceDir
  #define SourceDir "..\target\release"
#endif

#ifndef OutputDir
  #define OutputDir "..\release"
#endif

#ifndef VCRedistPath
  #define VCRedistPath "vc_redist.x64.exe"
#endif

[Setup]
AppId={{B8F5A5CA-4F4D-4F12-8C63-0A9F7E9E4D2A}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={localappdata}\Programs\EmulatorHub
AppendDefaultDirName=no
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=EmulatorHub-v{#MyAppVersion}-Setup
SetupIconFile=..\assets\eframe_icon.ico
UninstallDisplayIcon={app}\emulator_hub_gui.exe
Compression=lzma
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Files]
Source: "{#SourceDir}\emulator_hub_gui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#VCRedistPath}"; DestDir: "{tmp}"; Flags: deleteafterinstall
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Emulator Hub"; Filename: "{app}\emulator_hub_gui.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Emulator Hub"; Filename: "{app}\emulator_hub_gui.exe"; WorkingDir: "{app}"

[Run]
Filename: "{tmp}\vc_redist.x64.exe"; Parameters: "/install /quiet /norestart"; StatusMsg: "Installing Microsoft Visual C++ runtime..."; Flags: waituntilterminated
Filename: "{app}\emulator_hub_gui.exe"; Description: "Launch Emulator Hub"; Flags: nowait postinstall skipifsilent
