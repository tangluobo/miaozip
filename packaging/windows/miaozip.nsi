Unicode true
RequestExecutionLevel admin

!include "MUI2.nsh"

!define APP_NAME "妙压"
!define APP_PUBLISHER "MiaoZip Contributors"
!define APP_EXE "miaozip.exe"
!define APP_KEY "Software\tangluobo\MiaoZip"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\MiaoZip"

Name "${APP_NAME} ${VERSION}"
OutFile "${OUTPUT_FILE}"
BrandingText "${APP_NAME}"
SetCompressor /SOLID lzma

!if "${ARCHITECTURE}" == "x86"
  InstallDir "$PROGRAMFILES32\MiaoZip"
!else
  InstallDir "$PROGRAMFILES64\MiaoZip"
!endif
InstallDirRegKey HKLM "${APP_KEY}" "InstallDir"

!define MUI_ICON "${ICON_FILE}"
!define MUI_UNICON "${ICON_FILE}"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Section "MiaoZip" SEC_MAIN
  SetShellVarContext all
!if "${ARCHITECTURE}" != "x86"
  SetRegView 64
!endif
  SetOutPath "$INSTDIR"
  File "/oname=${APP_EXE}" "${SOURCE_DIR}\miaozip.exe"
  File "${SOURCE_DIR}\README.md"
  File "${SOURCE_DIR}\LICENSE"
  File "${SOURCE_DIR}\THIRD_PARTY_NOTICES.md"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\卸载${APP_NAME}.lnk" "$INSTDIR\Uninstall.exe"
  CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}"

  WriteRegStr HKLM "${APP_KEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "Publisher" "${APP_PUBLISHER}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\${APP_EXE},0"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetShellVarContext all
!if "${ARCHITECTURE}" != "x86"
  SetRegView 64
!endif
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.md"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$DESKTOP\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\卸载${APP_NAME}.lnk"
  RMDir "$INSTDIR"
  RMDir "$SMPROGRAMS\${APP_NAME}"
  DeleteRegKey HKLM "${APP_KEY}"
  DeleteRegKey HKLM "${UNINSTALL_KEY}"
SectionEnd
