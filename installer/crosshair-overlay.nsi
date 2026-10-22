Unicode true
ManifestDPIAware true
RequestExecutionLevel admin
SetCompressor /SOLID lzma
CRCCheck on

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "Sections.nsh"
!include "WinVer.nsh"
!include "x64.nsh"

!ifndef APP_VERSION
  !define APP_VERSION "0.1.0"
!endif
!ifndef APP_EXE
  !define APP_EXE "${__FILEDIR__}../target/release/crosshair-overlay.exe"
!endif
!ifndef ICON_FILE
  !define ICON_FILE "${__FILEDIR__}../assets/icon.ico"
!endif
!ifndef LICENSE_FILE
  !define LICENSE_FILE "${__FILEDIR__}../LICENSE"
!endif
!ifndef OUT_FILE
  !define OUT_FILE "${__FILEDIR__}../dist/CrosshairOverlay-Setup-${APP_VERSION}.exe"
!endif

!define APP_NAME "Crosshair Overlay"
!define APP_EXE_NAME "crosshair-overlay.exe"
!define APP_KEY "Software\CrosshairOverlay"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\CrosshairOverlay"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define RUN_VALUE "CrosshairOverlay"
!define START_MENU_FOLDER "Crosshair Overlay"

Name "${APP_NAME}"
Caption "${APP_NAME} ${APP_VERSION}"
BrandingText "${APP_NAME} ${APP_VERSION}"
OutFile "${OUT_FILE}"
InstallDir "$PROGRAMFILES64\Crosshair Overlay"
InstallDirRegKey HKLM "${APP_KEY}" "InstallDir"
Icon "${ICON_FILE}"
UninstallIcon "${ICON_FILE}"

VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey /LANG=1049 "ProductName" "${APP_NAME}"
VIAddVersionKey /LANG=1049 "CompanyName" "Crosshair Overlay"
VIAddVersionKey /LANG=1049 "FileDescription" "Установщик Crosshair Overlay"
VIAddVersionKey /LANG=1049 "FileVersion" "${APP_VERSION}"
VIAddVersionKey /LANG=1049 "ProductVersion" "${APP_VERSION}"
VIAddVersionKey /LANG=1049 "LegalCopyright" "MIT License"

!define MUI_ABORTWARNING
!define MUI_ICON "${ICON_FILE}"
!define MUI_UNICON "${ICON_FILE}"
!define MUI_WELCOMEPAGE_TITLE "Установка Crosshair Overlay"
!define MUI_WELCOMEPAGE_TEXT "Внешний статичный прицел для Windows.\r\n\r\nИгра должна работать в оконном или безрамочном режиме. Эксклюзивный полноэкранный режим не поддерживается.\r\n\r\nНажмите «Далее», чтобы продолжить."
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE_NAME}"
!define MUI_FINISHPAGE_RUN_TEXT "Запустить ${APP_NAME}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${LICENSE_FILE}"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "Russian"

LangString DESC_APP ${LANG_RUSSIAN} "Приложение, ярлык в меню «Пуск» и деинсталлятор."
LangString DESC_DESKTOP ${LANG_RUSSIAN} "Дополнительный ярлык на рабочем столе."
LangString DESC_AUTOSTART ${LANG_RUSSIAN} "Запускать приложение при входе в Windows. Настройку можно изменить в приложении."

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "Требуется 64-битная версия Windows."
    Abort
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_ICONSTOP "Требуется Windows 10 или новее."
    Abort
  ${EndIf}
  SetRegView 64
FunctionEnd

Function un.onInit
  SetRegView 64
FunctionEnd

Section "Приложение" SEC_APP
  SectionIn RO
  SetShellVarContext all
  SetOutPath "$INSTDIR"
  SetOverwrite on

  nsExec::Exec '"$SYSDIR\taskkill.exe" /IM ${APP_EXE_NAME}'
  Pop $0
  Sleep 600
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM ${APP_EXE_NAME}'
  Pop $0
  Sleep 300

  File "${APP_EXE}"
  File /oname=LICENSE.txt "${LICENSE_FILE}"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS\${START_MENU_FOLDER}"
  CreateShortcut "$SMPROGRAMS\${START_MENU_FOLDER}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE_NAME}" "" "$INSTDIR\${APP_EXE_NAME}" 0 SW_SHOWNORMAL "" "Внешний статичный прицел"
  CreateShortcut "$SMPROGRAMS\${START_MENU_FOLDER}\Удалить ${APP_NAME}.lnk" "$INSTDIR\Uninstall.exe" "" "$INSTDIR\Uninstall.exe" 0 SW_SHOWNORMAL "" "Удалить ${APP_NAME}"

  WriteRegStr HKLM "${APP_KEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "Publisher" "Crosshair Overlay"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\${APP_EXE_NAME}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKLM "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoRepair" 1
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "EstimatedSize" 7600
  AddSize 7600
SectionEnd

Section /o "Ярлык на рабочем столе" SEC_DESKTOP
  SetShellVarContext all
  CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE_NAME}" "" "$INSTDIR\${APP_EXE_NAME}" 0 SW_SHOWNORMAL "" "Внешний статичный прицел"
SectionEnd

Section "Запускать при входе в Windows" SEC_AUTOSTART
  WriteRegStr HKCU "${RUN_KEY}" "${RUN_VALUE}" '"$INSTDIR\${APP_EXE_NAME}"'
SectionEnd

Section "-Очистка автозапуска"
  ${IfNot} ${SectionIsSelected} ${SEC_AUTOSTART}
    DeleteRegValue HKCU "${RUN_KEY}" "${RUN_VALUE}"
  ${EndIf}
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_APP} $(DESC_APP)
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_DESKTOP} $(DESC_DESKTOP)
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_AUTOSTART} $(DESC_AUTOSTART)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  SetShellVarContext all
  nsExec::Exec '"$SYSDIR\taskkill.exe" /IM ${APP_EXE_NAME}'
  Pop $0
  Sleep 600
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM ${APP_EXE_NAME}'
  Pop $0
  Sleep 300

  Delete "$SMPROGRAMS\${START_MENU_FOLDER}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${START_MENU_FOLDER}\Удалить ${APP_NAME}.lnk"
  RMDir "$SMPROGRAMS\${START_MENU_FOLDER}"
  Delete "$DESKTOP\${APP_NAME}.lnk"

  DeleteRegValue HKCU "${RUN_KEY}" "${RUN_VALUE}"
  DeleteRegKey HKLM "${UNINSTALL_KEY}"
  DeleteRegKey HKLM "${APP_KEY}"

  RMDir /r "$INSTDIR"
SectionEnd
