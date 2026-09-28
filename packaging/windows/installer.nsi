; Prism NSIS Installer Script
; Produces a per-user, silent-compatible installer matching Prism's distribution specification.

Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!ifndef PRODUCT_NAME
  !define PRODUCT_NAME "Prism"
!endif

!ifndef PRODUCT_PUBLISHER
  !define PRODUCT_PUBLISHER "Prism"
!endif

!ifndef PRODUCT_WEB_SITE
  !define PRODUCT_WEB_SITE "https://github.com/Summpot/prism"
!endif

!ifndef VERSION
  !define VERSION "0.1.0"
!endif

!ifndef ARCH
  !define ARCH "x64"
!endif

!ifndef OUTFILE
  !define OUTFILE "Prism_${VERSION}_${ARCH}-setup.exe"
!endif

!ifndef BIN_PATH
  !define BIN_PATH "target\release\prism.exe"
!endif

!ifndef ICON_PATH
  !define ICON_PATH "crates\prism\icons\icon.ico"
!endif

Name "${PRODUCT_NAME}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Prism"
InstallDirRegKey HKCU "Software\Prism" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma

; Interface Configuration
!define MUI_ABORTWARNING
!define MUI_ICON "${ICON_PATH}"
!define MUI_UNICON "${ICON_PATH}"

; Installer Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\prism.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch ${PRODUCT_NAME}"
!insertmacro MUI_PAGE_FINISH

; Uninstaller Pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

; Language
!insertmacro MUI_LANGUAGE "English"

Section "Install" SecInstall
    SetOutPath "$INSTDIR"
    File "/oname=prism.exe" "${BIN_PATH}"
    
    ; Create uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"
    
    ; Record install directory
    WriteRegStr HKCU "Software\Prism" "InstallDir" "$INSTDIR"
    
    ; Shortcuts
    CreateDirectory "$SMPROGRAMS\Prism"
    CreateShortcut "$SMPROGRAMS\Prism\Prism.lnk" "$INSTDIR\prism.exe" "" "$INSTDIR\prism.exe" 0
    CreateShortcut "$SMPROGRAMS\Prism\Uninstall Prism.lnk" "$INSTDIR\uninstall.exe" "" "$INSTDIR\uninstall.exe" 0
    CreateShortcut "$DESKTOP\Prism.lnk" "$INSTDIR\prism.exe" "" "$INSTDIR\prism.exe" 0
    
    ; Register prism:// protocol
    WriteRegStr HKCU "Software\Classes\prism" "" "URL:Prism Protocol"
    WriteRegStr HKCU "Software\Classes\prism" "URL Protocol" ""
    WriteRegStr HKCU "Software\Classes\prism\DefaultIcon" "" "$INSTDIR\prism.exe,0"
    WriteRegStr HKCU "Software\Classes\prism\shell" "" "open"
    WriteRegStr HKCU "Software\Classes\prism\shell\open\command" "" '"$INSTDIR\prism.exe" "%1"'
    
    ; Register Add/Remove Programs uninstall entry
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.prism.client" "DisplayName" "${PRODUCT_NAME}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.prism.client" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.prism.client" "DisplayIcon" "$INSTDIR\prism.exe,0"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.prism.client" "Publisher" "${PRODUCT_PUBLISHER}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.prism.client" "HelpLink" "${PRODUCT_WEB_SITE}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.prism.client" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.prism.client" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.prism.client" "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.prism.client" "NoRepair" 1
SectionEnd

Section "Uninstall"
    ; Remove shortcuts
    Delete "$DESKTOP\Prism.lnk"
    Delete "$SMPROGRAMS\Prism\Prism.lnk"
    Delete "$SMPROGRAMS\Prism\Uninstall Prism.lnk"
    RMDir "$SMPROGRAMS\Prism"
    
    ; Remove registry entries
    DeleteRegKey HKCU "Software\Classes\prism"
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.prism.client"
    DeleteRegKey HKCU "Software\Prism"
    
    ; Remove installed files
    Delete "$INSTDIR\prism.exe"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"
SectionEnd
