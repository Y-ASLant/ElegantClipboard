Unicode true
ManifestDPIAware true
ManifestDPIAwareness PerMonitorV2

; 当前用户安装，对应 main 分支 Tauri 的 installMode=currentUser。
; 程序放到 %LOCALAPPDATA%\Programs\ElegantClipboard。
; %LOCALAPPDATA%\ElegantClipboard 是 GPUI 数据目录，不能作为安装目录。

!ifndef VERSION
  !error "必须传入 /DVERSION"
!endif
!ifndef VERSION_QUAD
  !error "必须传入 /DVERSION_QUAD"
!endif
!ifndef OUTFILE
  !error "必须传入 /DOUTFILE"
!endif
!ifndef SOURCE_EXE
  !error "必须传入 /DSOURCE_EXE"
!endif
!ifndef SOURCE_LICENSE
  !error "必须传入 /DSOURCE_LICENSE"
!endif
!ifndef SOURCE_README
  !error "必须传入 /DSOURCE_README"
!endif
!ifndef ICON
  !error "必须传入 /DICON"
!endif

SetCompressor /SOLID lzma

!include MUI2.nsh
!include LogicLib.nsh
!include x64.nsh
!include FileFunc.nsh
!include StrFunc.nsh

${Using:StrFunc} StrCase
${Using:StrFunc} UnStrCase
${Using:StrFunc} UnStrStr

!define PRODUCTNAME "ElegantClipboard"
!define MANUFACTURER "ASLant"
!define EXE "elegant-clipboard-gpui.exe"
!define HOMEPAGE "https://github.com/Y-ASLant/ElegantClipboard"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCTNAME}"
!define RUNKEY "Software\Microsoft\Windows\CurrentVersion\Run"

Name "${PRODUCTNAME}"
BrandingText "${PRODUCTNAME} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\${PRODUCTNAME}"
InstallDirRegKey HKCU "${UNINSTKEY}" "InstallLocation"
RequestExecutionLevel user

VIProductVersion "${VERSION_QUAD}"
VIAddVersionKey "ProductName" "${PRODUCTNAME}"
VIAddVersionKey "FileDescription" "${PRODUCTNAME}"
VIAddVersionKey "LegalCopyright" "Copyright ${MANUFACTURER}"
VIAddVersionKey "FileVersion" "${VERSION_QUAD}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "CompanyName" "${MANUFACTURER}"

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "$(finishRun)"
!define MUI_FINISHPAGE_SHOWREADME
!define MUI_FINISHPAGE_SHOWREADME_TEXT "$(createDesktop)"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION CreateDesktopShortcut

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${SOURCE_LICENSE}"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

LangString x64Only ${LANG_SIMPCHINESE} "ElegantClipboard 仅提供 64 位 Windows 安装包。"
LangString x64Only ${LANG_ENGLISH} "ElegantClipboard provides a 64-bit Windows installer only."
LangString dataDirCollision ${LANG_SIMPCHINESE} "安装目录与剪贴板数据目录相同。请先卸载指向该数据目录的旧安装，再重新安装。"
LangString dataDirCollision ${LANG_ENGLISH} "The installation folder is the clipboard data folder. Remove the old installation that points there, then install again."
LangString appRunning ${LANG_SIMPCHINESE} "ElegantClipboard 正在运行。继续将关闭此安装目录中的程序。"
LangString appRunning ${LANG_ENGLISH} "ElegantClipboard is running. Continuing will close the copy in this installation folder."
LangString closeFailed ${LANG_SIMPCHINESE} "无法关闭正在运行的 ElegantClipboard。请先从托盘退出，再继续。"
LangString closeFailed ${LANG_ENGLISH} "ElegantClipboard is still running. Quit it from the tray, then continue."
LangString finishRun ${LANG_SIMPCHINESE} "运行 ElegantClipboard"
LangString finishRun ${LANG_ENGLISH} "Run ElegantClipboard"
LangString createDesktop ${LANG_SIMPCHINESE} "创建桌面快捷方式"
LangString createDesktop ${LANG_ENGLISH} "Create a desktop shortcut"

!macro SelectInstallerLanguage
  System::Call 'kernel32::GetUserDefaultUILanguage() i .r0'
  IntOp $1 $0 & 0x3FF
  ${If} $1 = 4
    StrCpy $LANGUAGE ${LANG_SIMPCHINESE}
  ${Else}
    StrCpy $LANGUAGE ${LANG_ENGLISH}
  ${EndIf}
!macroend

!macro RejectDataDirectory
  ${StrCase} $1 "$INSTDIR" "L"
  ${StrCase} $2 "$LOCALAPPDATA\ElegantClipboard" "L"
  ${If} $1 == $2
    MessageBox MB_ICONSTOP "$(dataDirCollision)"
    Abort
  ${EndIf}
!macroend

!macro CloseRunningCopy Prefix Unique
  Function ${Prefix}CloseRunningCopy
    ${IfNot} ${FileExists} "$INSTDIR\${EXE}"
      Return
    ${EndIf}
    System::Call 'Kernel32::SetEnvironmentVariable(t "ELEGANT_CLIPBOARD_EXE", t "$INSTDIR\${EXE}")'
    nsExec::ExecToStack 'powershell.exe -NoProfile -NonInteractive -Command "if (Get-Process -Name elegant-clipboard-gpui -ErrorAction SilentlyContinue | Where-Object { $$_.Path -eq $$env:ELEGANT_CLIPBOARD_EXE }) { exit 10 } else { exit 0 }"'
    Pop $0
    Pop $1
    ${If} $0 = 0
      Return
    ${EndIf}
    ${If} $0 = 10
      IfSilent kill${Unique} ask${Unique}
      ask${Unique}:
        MessageBox MB_OKCANCEL|MB_ICONEXCLAMATION "$(appRunning)" IDOK kill${Unique}
        Abort
      kill${Unique}:
        nsExec::ExecToStack 'powershell.exe -NoProfile -NonInteractive -Command "Get-Process -Name elegant-clipboard-gpui -ErrorAction SilentlyContinue | Where-Object { $$_.Path -eq $$env:ELEGANT_CLIPBOARD_EXE } | Stop-Process -Force; Start-Sleep -Milliseconds 500; if (Get-Process -Name elegant-clipboard-gpui -ErrorAction SilentlyContinue | Where-Object { $$_.Path -eq $$env:ELEGANT_CLIPBOARD_EXE }) { exit 1 } else { exit 0 }"'
        Pop $0
        Pop $1
        ${If} $0 <> 0
          IfSilent fail${Unique} show${Unique}
          show${Unique}:
            MessageBox MB_ICONSTOP "$(closeFailed)"
          fail${Unique}:
            Abort
        ${EndIf}
        Return
    ${EndIf}
    MessageBox MB_ICONSTOP "$(closeFailed)"
    Abort
  FunctionEnd
!macroend

!insertmacro CloseRunningCopy "" Inst
!insertmacro CloseRunningCopy "un." Uninst

Function CreateDesktopShortcut
  CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${EXE}"
FunctionEnd

Function .onInit
  SetShellVarContext current
  SetRegView 64
  !insertmacro SelectInstallerLanguage
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "$(x64Only)"
    Abort
  ${EndIf}
  !insertmacro RejectDataDirectory
FunctionEnd

Function un.onInit
  SetShellVarContext current
  SetRegView 64
  !insertmacro SelectInstallerLanguage
FunctionEnd

Section
  Call CloseRunningCopy
  !insertmacro RejectDataDirectory
  SetOutPath "$INSTDIR"
  File /oname=${EXE} "${SOURCE_EXE}"
  File /oname=LICENSE "${SOURCE_LICENSE}"
  File /oname=README.txt "${SOURCE_README}"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  WriteRegStr HKCU "${UNINSTKEY}" "DisplayName" "${PRODUCTNAME}"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayIcon" "$INSTDIR\${EXE}"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTKEY}" "Publisher" "${MANUFACTURER}"
  WriteRegStr HKCU "${UNINSTKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegStr HKCU "${UNINSTKEY}" "URLInfoAbout" "${HOMEPAGE}"
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINSTKEY}" "EstimatedSize" "$0"

  CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${EXE}"
  ${If} ${Silent}
    Call CreateDesktopShortcut
  ${EndIf}
SectionEnd

Function un.RemoveAutostart
  StrCpy $0 0
  loop:
    EnumRegValue $1 HKCU "${RUNKEY}" $0
    StrCmp $1 "" done
    ReadRegStr $2 HKCU "${RUNKEY}" $1
    ${UnStrCase} $4 $2 "L"
    ${UnStrCase} $5 "$INSTDIR\${EXE}" "L"
    ${UnStrStr} $3 $4 $5
    StrCmp $3 "" next
      DeleteRegValue HKCU "${RUNKEY}" $1
      StrCpy $0 0
      Goto loop
    next:
      IntOp $0 $0 + 1
      Goto loop
  done:
FunctionEnd

Section "Uninstall"
  Call un.CloseRunningCopy
  Call un.RemoveAutostart
  Delete "$INSTDIR\${EXE}"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\README.txt"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\${PRODUCTNAME}.lnk"
  Delete "$DESKTOP\${PRODUCTNAME}.lnk"
  Delete "$SMPROGRAMS\${PRODUCTNAME}\${PRODUCTNAME}.lnk"
  RMDir "$SMPROGRAMS\${PRODUCTNAME}"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "${UNINSTKEY}"
SectionEnd
