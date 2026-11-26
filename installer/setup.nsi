; -*- coding: utf-8 -*-
Unicode true
!include "MUI2.nsh"
!include "nsDialogs.nsh"
!include "x64.nsh"
!include "WinVer.nsh"
!include "FileFunc.nsh"

!ifndef APP_ID
  !define APP_ID "Kuidao"
!endif
!ifndef APP_NAME
  !define APP_NAME "鬼島 Kuidao 桌面靈動島"
!endif
!ifndef BOOTSTRAP_EXE
  !error "BOOTSTRAP_EXE is required"
!endif
!ifndef DISPLAY_VERSION
  !define DISPLAY_VERSION "0.6.00"
!endif
!ifndef DISPLAY_VERSION_NUM
  !define DISPLAY_VERSION_NUM "0.6.0.0"
!endif
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"
Name "${APP_NAME}"
OutFile "${OUTPUT_FILE}"
InstallDir "$PROGRAMFILES64\${APP_ID}"
InstallDirRegKey HKLM "${UNINSTALL_KEY}" "InstallLocation"
RequestExecutionLevel admin
ManifestDPIAware true
SetCompressor /SOLID lzma
VIProductVersion "0.6.70.0"
VIAddVersionKey /LANG=1028 "ProductName" "${APP_NAME}"
VIAddVersionKey /LANG=1028 "FileDescription" "${APP_NAME}安裝程式"
VIAddVersionKey /LANG=1028 "FileVersion" "${DISPLAY_VERSION}"
VIAddVersionKey /LANG=1028 "LegalCopyright" "鬼島貢獻者"
!define MUI_ICON "${PROJECT_DIR}\assets\kuidao.ico"
!define MUI_UNICON "${PROJECT_DIR}\assets\kuidao.ico"
!define MUI_ABORTWARNING
Var BrandIcon
Var PathInput
Var DesktopCheckbox
Var AutoStartCheckbox
Var DesktopChoice
Var AutoStartChoice
Page custom InstallPage InstallPageLeave
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_TITLE "鬼島已安裝"
!define MUI_FINISHPAGE_TEXT "安裝包只放入極小啟動器。按下完成後會自動下載主程式，下載完成便會開啟鬼島。"
!define MUI_FINISHPAGE_RUN "$INSTDIR\kuidao-bootstrap.exe"
!define MUI_FINISHPAGE_RUN_PARAMETERS "--update"
!define MUI_FINISHPAGE_RUN_TEXT "下載並開啟鬼島"
!insertmacro MUI_PAGE_FINISH
!define MUI_UNCONFIRMPAGE_TEXT_TOP "將解除安裝鬼島。設定、待辦和連線記錄會保留，重新安裝後仍可使用。"
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "TradChinese"
BrandingText " "
SetFont "Microsoft YaHei UI" 9

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
  SetShellVarContext all
  SetRegView 64
  ; 靜默升級沒有 /D 時，InstallDirRegKey 可能因登錄视图時機讀不到 64 位元路徑；
  ; 這裡在設定 64 位元檢視後再讀一次，只有目前仍是預設目錄才沿用舊安裝位置。
  ReadRegStr $0 HKLM "${UNINSTALL_KEY}" "InstallLocation"
  ${If} $0 == ""
    ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}" "InstallLocation"
  ${EndIf}
  ${If} $0 != ""
  ${AndIf} $INSTDIR == "$PROGRAMFILES64\${APP_ID}"
    StrCpy $INSTDIR $0
  ${EndIf}
  StrCpy $DesktopChoice 1
  StrCpy $AutoStartChoice 0
  ReadRegStr $0 HKLM "${UNINSTALL_KEY}" "InstallLocation"
  ${If} $0 == ""
    ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}" "InstallLocation"
  ${EndIf}
  ${If} $0 != ""
    IfFileExists "$DESKTOP\${APP_NAME}.lnk" +2 0
      StrCpy $DesktopChoice 0
  ${EndIf}
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${APP_ID}"
  ${If} $0 != ""
    StrCpy $AutoStartChoice 1
  ${EndIf}
  ; 靜默安裝使用同一組選項；/D 路徑由 NSIS 自身處理。
  ${GetParameters} $0
  ${GetOptions} $0 "/DESKTOP=" $1
  ${If} $1 == "0"
  ${OrIf} $1 == "1"
    StrCpy $DesktopChoice $1
  ${EndIf}
  ${GetOptions} $0 "/AUTOSTART=" $1
  ${If} $1 == "0"
  ${OrIf} $1 == "1"
    StrCpy $AutoStartChoice $1
  ${EndIf}
FunctionEnd

Function InstallPage
  !insertmacro MUI_HEADER_TEXT "歡迎使用鬼島" "Windows 頂部的桌面夥伴"
  nsDialogs::Create 1018
  Pop $0
  InitPluginsDir
  File /oname=$PLUGINSDIR\brand.ico "${PROJECT_DIR}\assets\kuidao.ico"
  ${NSD_CreateIcon} 0 6u 44u 44u ""
  Pop $0
  System::Call 'user32::LoadImageW(p 0, w "$PLUGINSDIR\brand.ico", i 1, i 64, i 64, i 0x10) p .r1'
  StrCpy $BrandIcon $1
  SendMessage $0 0x170 $1 0
  ${NSD_CreateLabel} 58u 6u 240u 24u "${APP_NAME}"
  Pop $0
  CreateFont $1 "Microsoft YaHei UI" 18 700
  SendMessage $0 ${WM_SETFONT} $1 0
  ; 版本與標題分開兩行，避免 18pt 標題和版本字疊在一起被截斷。
  ${NSD_CreateLabel} 59u 31u 240u 12u "v${DISPLAY_VERSION} · 安裝後自動補齊主程式"
  Pop $0
  SetCtlColors $0 0x666666 0xF0F0F0
  ${NSD_CreateLabel} 58u 45u 240u 16u "先安裝極小啟動器，不一次下載完整安裝包。"
  Pop $0
  ${NSD_CreateLabel} 0 64u 300u 16u "音樂歌詞 · 郵件驗證碼 · 行程提醒 · AI 額度"
  Pop $0
  ${NSD_CreateLabel} 0 84u 300u 12u "安裝位置"
  Pop $0
  ${NSD_CreateDirRequest} 0 100u 245u 16u "$INSTDIR"
  Pop $PathInput
  ${NSD_CreateBrowseButton} 250u 100u 50u 16u "瀏覽…"
  Pop $0
  ${NSD_OnClick} $0 BrowseDirectory
  ${NSD_CreateCheckbox} 0 126u 150u 14u "建立桌面捷徑"
  Pop $DesktopCheckbox
  ${NSD_SetState} $DesktopCheckbox $DesktopChoice
  ${NSD_CreateCheckbox} 165u 126u 135u 14u "開機自啟"
  Pop $AutoStartCheckbox
  ${NSD_SetState} $AutoStartCheckbox $AutoStartChoice
  GetDlgItem $0 $HWNDPARENT 1
  SendMessage $0 ${WM_SETTEXT} 0 "STR:一鍵安裝"
  nsDialogs::Show
FunctionEnd

; 只接受磁碟機代號或 UNC 共享下的完整子目錄。先規範化，避免把空輸入或磁碟機代號補成安裝路徑。
Function CheckFolderPath
  ${GetRoot} "$INSTDIR" $0
  ${If} $0 != ""
    StrLen $1 $0
    StrCpy $2 "$INSTDIR" "" $1
    StrCpy $1 $2 1
    ${If} $1 == "\"
    ${AndIf} $2 != "\"
      System::Call 'kernel32::GetFullPathNameW(w "$INSTDIR", i ${NSIS_MAX_STRLEN}, w .r2, p 0) i .r0'
      ${If} $0 == 0
      ${OrIf} $0 >= ${NSIS_MAX_STRLEN}
        Goto invalid_path
      ${EndIf}
      System::Call 'shlwapi::PathIsRootW(w "$2") i .r0'
      ${If} $0 == 0
        StrCpy $INSTDIR $2
        StrCpy $0 1
        Return
      ${EndIf}
    ${EndIf}
  ${EndIf}
  invalid_path:
  StrCpy $0 0
FunctionEnd

; 選取的資料夾裡已有別的東西（又不是鬼島自己的）時，在其中新建 Kuidao 子資料夾，
; 避免把整個共享資料夾登記成鬼島的安裝位置（軟體管家會把整個資料夾的大小算到鬼島頭上）。
; 資料夾本身叫 Kuidao、裡面已有鬼島（原地升級）、不存在或為空（使用者特意準備的）時原樣使用。
Function EnsureOwnFolder
  Call CheckFolderPath
  ${If} $0 == 0
    Return
  ${EndIf}
  StrCpy $0 $INSTDIR 1 -1
  ${If} $0 == "\"
    StrCpy $INSTDIR $INSTDIR -1
  ${EndIf}
  ${GetFileName} "$INSTDIR" $0
  ${If} $0 == "${APP_ID}"
    Return
  ${EndIf}
  ${If} ${FileExists} "$INSTDIR\kuidao.exe"
    Return
  ${EndIf}
  ${If} ${FileExists} "$INSTDIR\kuidao-bootstrap.exe"
    Return
  ${EndIf}
  ClearErrors
  FindFirst $1 $2 "$INSTDIR\*.*"
  ${DoWhile} $2 != ""
    ${If} $2 != "."
    ${AndIf} $2 != ".."
      FindClose $1
      StrCpy $INSTDIR "$INSTDIR\${APP_ID}"
      Return
    ${EndIf}
    FindNext $1 $2
  ${Loop}
  FindClose $1
  ClearErrors
FunctionEnd

Function BrowseDirectory
  Pop $0
  ${NSD_GetText} $PathInput $0
  nsDialogs::SelectFolderDialog "選擇鬼島安裝資料夾" "$0"
  Pop $0
  ${If} $0 != "error"
    StrCpy $INSTDIR $0
    Call EnsureOwnFolder
    ${NSD_SetText} $PathInput "$INSTDIR"
  ${EndIf}
FunctionEnd

Function ValidateDirectory
  Call CheckFolderPath
  ${If} $0 == 0
    Goto invalid
  ${EndIf}
  ClearErrors
  CreateDirectory "$INSTDIR"
  IfErrors invalid
  GetFullPathName $INSTDIR "$INSTDIR"
  IfErrors invalid
  GetTempFileName $0 "$INSTDIR"
  IfErrors invalid
  Delete "$0"
  SetErrorLevel 0
  Return
  invalid:
    MessageBox MB_OK|MB_ICONEXCLAMATION "請選擇可寫入的完整安裝資料夾，例如 D:\Apps\Kuidao。不要選擇磁碟根目錄或需要管理員許可權的資料夾。" /SD IDOK
    SetErrorLevel 1
    Abort
FunctionEnd

Function InstallPageLeave
  ${NSD_GetText} $PathInput $INSTDIR
  ${NSD_GetState} $DesktopCheckbox $DesktopChoice
  ${NSD_GetState} $AutoStartCheckbox $AutoStartChoice
  Call EnsureOwnFolder
  ${NSD_SetText} $PathInput "$INSTDIR"
  Call ValidateDirectory
FunctionEnd

Function .onGUIEnd
  System::Call 'user32::DestroyIcon(p $BrandIcon)'
FunctionEnd

; 安裝／解除安裝前先強制結束主程式與啟動器，避免檔案被鎖住；最多重試 3 次。
!macro ClosedCheck Prefix
Function ${Prefix}CheckClosed
  StrCpy $1 0
  kill_retry:
  nsExec::ExecToLog 'taskkill /F /T /IM kuidao.exe /IM kuidao-bootstrap.exe'
  Pop $0
  Sleep 500
  System::Call 'kernel32::OpenMutexW(i 0x100000, i 0, w "Local\${APP_ID}.SingleInstance") p .r0'
  ${If} $0 != 0
    System::Call 'kernel32::CloseHandle(p r0)'
    IntOp $1 $1 + 1
    ${If} $1 < 3
      Goto kill_retry
    ${Else}
      MessageBox MB_OK|MB_ICONSTOP "無法關閉${APP_NAME}，請重新開機後再執行安裝。" /SD IDOK
      SetErrorLevel 3
      Abort
    ${EndIf}
  ${EndIf}
FunctionEnd
!macroend
!insertmacro ClosedCheck ""
!insertmacro ClosedCheck "un."

Section "安裝"
  Call CheckClosed
  ; 靜默安裝的 /D 路徑同樣遵循專用資料夾規則。
  Call EnsureOwnFolder
  Call ValidateDirectory
  SetOutPath "$INSTDIR"
  SetOverwrite on
  File /oname=kuidao-bootstrap.exe "${BOOTSTRAP_EXE}"
  File "${PROJECT_DIR}\LICENSE"
  File "${PROJECT_DIR}\THIRD_PARTY_NOTICES.txt"
  ; 診斷指令碼已內嵌在程式裡（kuidao.exe --collect-diagnostics），清掉測試版留下的單獨診斷檔案。
  Delete "$INSTDIR\收集鬼島診斷.cmd"
  Delete "$INSTDIR\scripts\collect-diagnostics.ps1"
  RMDir "$INSTDIR\scripts"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  ${If} $DesktopChoice == 1
    CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\kuidao-bootstrap.exe"
  ${Else}
    Delete "$DESKTOP\${APP_NAME}.lnk"
  ${EndIf}
  ${If} $AutoStartChoice == 1
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${APP_ID}" '"$INSTDIR\kuidao.exe"'
  ${Else}
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${APP_ID}"
  ${EndIf}
  CreateShortcut "$SMPROGRAMS\${APP_NAME}.lnk" "$INSTDIR\kuidao-bootstrap.exe"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}設定.lnk" "$INSTDIR\kuidao-bootstrap.exe" "--settings"
  ; 鬼島打不開、卡死時的備用入口；主程式補齊後由啟動器轉交診斷參數。
  CreateShortcut "$SMPROGRAMS\收集${APP_NAME}診斷.lnk" "$INSTDIR\kuidao-bootstrap.exe" "--collect-diagnostics" "$INSTDIR\kuidao-bootstrap.exe" 0
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayName" "${APP_NAME}（Kuidao）"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayVersion" "${DISPLAY_VERSION}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "Publisher" "鬼島"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "URLInfoAbout" "https://github.com/"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "HelpLink" "https://github.com/"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\kuidao-bootstrap.exe,0"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKLM "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoRepair" 1
  ; 登記鬼島自己檔案的大小（KB），軟體管家就不會去統計整個安裝資料夾。
  ${GetSize} "$INSTDIR" "/M=kuidao-bootstrap.exe /S=0K /G=0" $0 $1 $2
  ${GetSize} "$INSTDIR" "/M=Uninstall.exe /S=0K /G=0" $3 $1 $2
  IntOp $0 $0 + $3
  ${GetSize} "$INSTDIR" "/M=THIRD_PARTY_NOTICES.txt /S=0K /G=0" $3 $1 $2
  IntOp $0 $0 + $3
  IntOp $0 $0 + 8
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "EstimatedSize" $0
  Delete "$INSTDIR\uninstall.ps1"
SectionEnd

Function un.onInit
  SetShellVarContext all
  SetRegView 64
FunctionEnd

Section "Uninstall"
  Call un.CheckClosed
  Delete "$INSTDIR\kuidao.exe"
  Delete "$INSTDIR\kuidao-bootstrap.exe"
  Delete "$INSTDIR\kuidao-previous.exe"
  Delete "$INSTDIR\kuidao-v0.2-backup.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.txt"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\uninstall.ps1"
  Delete "$INSTDIR\收集鬼島診斷.cmd"
  Delete "$INSTDIR\scripts\collect-diagnostics.ps1"
  RMDir /r "$INSTDIR\assets"
  RMDir /r "$INSTDIR\scripts"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$DESKTOP\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}設定.lnk"
  Delete "$SMPROGRAMS\收集${APP_NAME}診斷.lnk"
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${APP_ID}"
  ${If} $0 == '"$INSTDIR\kuidao.exe"'
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${APP_ID}"
  ${EndIf}
  DeleteRegKey HKLM "${UNINSTALL_KEY}"
  ; 只移除空的安裝目錄，使用者資料和使用者自行放入的檔案都保留。
  RMDir "$INSTDIR"
SectionEnd


