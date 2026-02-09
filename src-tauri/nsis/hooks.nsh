; NSIS hooks for ElegantClipboard installer
; Kill running instance before installing to prevent "file in use" errors

!macro NSIS_HOOK_PREINSTALL
  ; Step 1: Try normal kill (works for non-admin instances)
  nsExec::ExecToLog 'taskkill /F /T /IM "elegant-clipboard.exe"'
  Sleep 500

  ; Step 2: If process is still running (likely admin-launched), try elevated kill
  ; PowerShell checks if process still exists, only shows UAC prompt when needed
  nsExec::ExecToLog `powershell -NoProfile -Command "if(Get-Process 'elegant-clipboard' -EA 0){Start-Process taskkill -ArgumentList '/F','/T','/IM','elegant-clipboard.exe' -Verb RunAs -Wait -WindowStyle Hidden -EA 0}"`
  Sleep 500
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Same as PREINSTALL: kill running instance before uninstalling
  nsExec::ExecToLog 'taskkill /F /T /IM "elegant-clipboard.exe"'
  Sleep 500

  nsExec::ExecToLog `powershell -NoProfile -Command "if(Get-Process 'elegant-clipboard' -EA 0){Start-Process taskkill -ArgumentList '/F','/T','/IM','elegant-clipboard.exe' -Verb RunAs -Wait -WindowStyle Hidden -EA 0}"`
  Sleep 500
!macroend
