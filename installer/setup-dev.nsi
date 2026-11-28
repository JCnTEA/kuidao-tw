; -*- coding: utf-8 -*-
; 開發版專用安裝檔：每位使用者自己安裝，不需要管理員權限。
; 只放進極小的開發版啟動器（KuidaoDevLauncher.exe），第一次開啟會下載最新的
; 開發版發布包（程式＋素材），之後每次啟動都比版本、自動換新版。
Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
!include "WinVer.nsh"
!include "FileFunc.nsh"

!ifndef BOOTSTRAP_EXE
  !error "BOOTSTRAP_EXE is required"
!endif
!ifndef DISPLAY_VERSION
  !define DISPLAY_VERSION "0.7.9"
!endif
!ifndef DISPLAY_VERSION_NUM
  !define DISPLAY_VERSION_NUM "0.7.9.0"
!endif
!ifndef OUTPUT_FILE
  !define OUTPUT_FILE "KuidaoDev-Setup-TW-x64.exe"
!endif

!define APP_ID "KuidaoDev"
!define APP_NAME "鬼島 Kuidao 開發版"
!define LAUNCHER_NAME "KuidaoDevLauncher.exe"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"

Name "${APP_NAME}"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\${APP_ID}"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
RequestExecutionLevel user
ManifestDPIAware true
SetCompressor /SOLID lzma
VIProductVersion "${DISPLAY_VERSION_NUM}"
VIAddVersionKey /LANG=1028 "ProductName" "${APP_NAME}"
VIAddVersionKey /LANG=1028 "FileDescription" "${APP_NAME}安裝程式"
VIAddVersionKey /LANG=1028 "FileVersion" "${DISPLAY_VERSION}"
VIAddVersionKey /LANG=1028 "LegalCopyright" "鬼島貢獻者"
!define MUI_ICON "${PROJECT_DIR}\assets\kuidao.ico"
!define MUI_UNICON "${PROJECT_DIR}\assets\kuidao.ico"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "${APP_NAME}"
!define MUI_WELCOMEPAGE_TEXT "開發版安裝在自己的使用者資料夾（%LOCALAPPDATA%\KuidaoDev），不需要管理員權限，也不會跟正式版互相蓋掉。$\r$\n$\r$\n安裝包只有一個極小的啟動器；按下完成後會自動抓最新的開發版，之後每次開啟都會先確認版本、自動更新程式與素材。"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_TITLE "開發版啟動器已安裝"
!define MUI_FINISHPAGE_TEXT "啟動器會下載最新的開發版（程式與素材），下載完成便會開啟鬼島開發版。版本只差一版時會開更新視窗；差距更大會自動重啟更新。每天 00:00 也會檢查，電腦關機後開機同樣會處理。"
!define MUI_FINISHPAGE_RUN "$INSTDIR\${LAUNCHER_NAME}"
!define MUI_FINISHPAGE_RUN_PARAMETERS "--update"
!define MUI_FINISHPAGE_RUN_TEXT "下載並開啟開發版"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "TradChinese"
BrandingText " "

; 正式版（kuidao.exe／kuidao-bootstrap.exe）可能在背景執行且常駐在島上，
; 開發版要安裝就把正式版先關掉：只結束程序，不會移除正式版的安裝內容與設定。
Function CloseOfficial
  StrCpy $1 0
  nsExec::ExecToStack 'taskkill /F /T /IM kuidao.exe'
  Pop $0
  ${If} $0 == 0
    StrCpy $1 1
  ${EndIf}
  nsExec::ExecToStack 'taskkill /F /T /IM kuidao-bootstrap.exe'
  Pop $0
  ${If} $0 == 0
    StrCpy $1 1
  ${EndIf}
  ${If} $1 == 1
    Sleep 400
    MessageBox MB_OK|MB_ICONINFORMATION "偵測到正式版正在執行，已先關閉正式版。$\r$\n$\r$\n正式版的安裝檔案與設定都會保留，之後仍可繼續使用正式版。"
  ${EndIf}
FunctionEnd

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "鬼島需要 Windows 10 或 Windows 11 的 64 位系統。" /SD IDOK
    SetErrorLevel 1
    Quit
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_OK|MB_ICONSTOP "鬼島需要 Windows 10 或 Windows 11。" /SD IDOK
    SetErrorLevel 1
    Quit
  ${EndIf}
  SetShellVarContext current
  SetRegView 64
  Call CloseOfficial
FunctionEnd

Section "Install"
  ; 開發版與正式版可以同時存在：只關掉開發版自己的程式。
  nsExec::ExecToLog 'taskkill /F /T /IM "${APP_NAME}.exe" /IM ${LAUNCHER_NAME}'
  Sleep 300
  SetOutPath "$INSTDIR"
  File /oname=${LAUNCHER_NAME} "${BOOTSTRAP_EXE}"
  File "${PROJECT_DIR}\LICENSE"
  File "${PROJECT_DIR}\THIRD_PARTY_NOTICES.txt"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${LAUNCHER_NAME}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}.lnk" "$INSTDIR\${LAUNCHER_NAME}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}解除安裝.lnk" "$INSTDIR\Uninstall.exe"

  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${DISPLAY_VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "鬼島"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\${LAUNCHER_NAME},0"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  nsExec::ExecToLog 'schtasks /Delete /TN "KuidaoDevMidnightUpdate" /F'
  nsExec::ExecToLog 'taskkill /F /T /IM "${APP_NAME}.exe" /IM ${LAUNCHER_NAME}'
  Sleep 300
  Delete "$INSTDIR\${LAUNCHER_NAME}"
  Delete "$INSTDIR\${APP_NAME}.exe"
  Delete "$INSTDIR\dev-version.txt"
  Delete "$INSTDIR\installed-version.txt"
  Delete "$INSTDIR\midnight-update-task.txt"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.txt"
  RMDir /r "$INSTDIR\assets"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$DESKTOP\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}解除安裝.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  ; 設定與歌詞庫留在 %APPDATA%\Kuidao，重新安裝後仍可使用。
  RMDir "$INSTDIR"
SectionEnd
