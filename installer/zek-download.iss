; Instalador do Zek Download (Inno Setup 6). Instala só para o usuário atual: não pede administrador.
; Compilar:  ISCC /DAppVersion=1.1.0 installer\zek-download.iss
#ifndef AppVersion
  #define AppVersion "1.1.0"
#endif

[Setup]
AppId={{8F3C2B1A-5D7E-4C9A-9B6E-2A1F0D4C7E55}
AppName=Zek Download
AppVersion={#AppVersion}
AppPublisher=aaaiiicerol
AppPublisherURL=https://github.com/aaaiiicerol
DefaultDirName={localappdata}\Programs\ZekDownload
DefaultGroupName=Zek Download
PrivilegesRequired=lowest
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=ZekDownload-Setup-{#AppVersion}
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\zek-download.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
ArchitecturesAllowed=x64compatible

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Tasks]
Name: "desktopicon"; Description: "Criar um atalho na área de trabalho"; GroupDescription: "Atalhos:"

[Files]
Source: "..\target\release\zek-download.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Zek Download"; Filename: "{app}\zek-download.exe"
Name: "{autodesktop}\Zek Download"; Filename: "{app}\zek-download.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\zek-download.exe"; Description: "Abrir o Zek Download"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Remove só o motor baixado (yt-dlp/ffmpeg/Deno). Suas configurações e downloads ficam.
Type: filesandordirs; Name: "{localappdata}\ZekDownload\bin"
