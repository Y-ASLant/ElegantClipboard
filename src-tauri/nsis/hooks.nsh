; NSIS 钩子：安装/卸载前终止正在运行的实例
; 用 cmd + findstr 快速检查进程，仅在需要时才调用 PowerShell 提权终止

!macro NSIS_HOOK_PREINSTALL
  ; 常规终止（非管理员实例）
  nsExec::ExecToLog 'taskkill /F /T /IM "elegant-clipboard.exe"'
  ; 仅在进程仍存在时提权终止（管理员实例），避免 PowerShell 冷启动延迟
  nsExec::ExecToLog `cmd /c tasklist /FI "IMAGENAME eq elegant-clipboard.exe" /NH 2>nul | findstr /I "elegant-clipboard" >nul 2>&1 && powershell -NoProfile -Command "Start-Process taskkill -ArgumentList '/F','/T','/IM','elegant-clipboard.exe' -Verb RunAs -Wait -WindowStyle Hidden"`
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog 'taskkill /F /T /IM "elegant-clipboard.exe"'
  nsExec::ExecToLog `cmd /c tasklist /FI "IMAGENAME eq elegant-clipboard.exe" /NH 2>nul | findstr /I "elegant-clipboard" >nul 2>&1 && powershell -NoProfile -Command "Start-Process taskkill -ArgumentList '/F','/T','/IM','elegant-clipboard.exe' -Verb RunAs -Wait -WindowStyle Hidden"`
!macroend
