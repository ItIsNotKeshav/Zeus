; Zeus's Windows installer, built from a release binary with Inno Setup 6.3
; or newer:
;
;   iscc /DVersion=0.19.0 /DArch=x86_64 /DBinary=...\zeus.exe ^
;        /DOutputDir=dist packaging\windows\zeus.iss
;
; Its own AppId, folder and shortcuts, so it installs beside ZapFast and
; never replaces it. Installation uses the current user's Programs folder
; and does not need administrator rights. Reinstalling closes a running
; copy before replacing it.

#ifndef Version
  #error Version must be defined on the ISCC command line
#endif
#ifndef Arch
  #error Arch must be defined on the ISCC command line (x86_64 or aarch64)
#endif
#ifndef NumericVersion
  #define NumericVersion Version
#endif
#ifndef Binary
  #error Binary must be defined on the ISCC command line
#endif
#ifndef OutputDir
  #error OutputDir must be defined on the ISCC command line
#endif
#if Arch == "aarch64"
  #define InnoArch "arm64"
#else
  #define InnoArch "x64compatible"
#endif

#define AppName "Zeus"
#define AppExeName "zeus.exe"

[Setup]
; Never change: this is how Windows tells an update from a new program.
AppId={{CB6A895A-4B29-47D8-96FC-B8C6181273A9}
AppName={#AppName}
AppVersion={#Version}
AppVerName={#AppName} {#Version}
AppPublisher=Keshav
AppCopyright=© 2026 Carmine Paolino and Keshav
AppPublisherURL=https://github.com/ItIsNotKeshav/zapfast
AppSupportURL=https://github.com/ItIsNotKeshav/zapfast/issues
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed={#InnoArch}
ArchitecturesInstallIn64BitMode={#InnoArch}
MinVersion=10.0
LicenseFile=..\..\LICENSE
OutputDir={#OutputDir}
OutputBaseFilename=zeus-{#Version}-{#Arch}-setup
SetupIconFile=zeus.ico
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\{#AppExeName}
VersionInfoVersion={#NumericVersion}.0

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "{#Binary}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\THIRD-PARTY-NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
; The fonts built into the program travel with their licences.
Source: "..\..\assets\fonts\*-OFL.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "..\..\assets\fonts\*-LICENSE.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"; AppUserModelID: "app.zeus.Zeus"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon; AppUserModelID: "app.zeus.Zeus"

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent
