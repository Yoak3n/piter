!include "LogicLib.nsh"

; Runs before Tauri removes the installed files, registry keys and shortcuts.
!macro NSIS_HOOK_PREUNINSTALL
  ; The app hides to tray on close, so the main process may still be running
  ; during uninstall. Kill it (and any pi child processes) first, otherwise
  ; their files are locked and cannot be removed.
  DetailPrint "Stopping piter and pi processes..."
  nsExec::ExecToLog '"$SYSDIR\taskkill.exe" /F /T /IM piter.exe'
  nsExec::ExecToLog '"$SYSDIR\taskkill.exe" /F /T /IM pi.exe'
  Sleep 1200

  ; The pi runtime directory is downloaded at runtime and is NOT part of the
  ; NSIS file manifest, so Tauri's uninstaller leaves it behind. Remove it.
  DetailPrint "Removing pi runtime directory..."
  RMDir /r "$INSTDIR\resources\pi"
  RMDir "$INSTDIR\resources"
!macroend

; Runs after Tauri removed the installed files, registry keys and shortcuts.
!macro NSIS_HOOK_POSTUNINSTALL
  ; When the user opted into deleting app data, also remove the real data
  ; directory. Tauri's built-in cleanup only removes $APPDATA\${BUNDLEID}
  ; (com.yoa.piter) while the app stores its data in $APPDATA\piter.
  ${If} $DeleteAppDataCheckboxState = 1
    DetailPrint "Removing app data directory..."
    RMDir /r "$APPDATA\piter"
    RMDir /r "$LOCALAPPDATA\piter"
  ${EndIf}

  ; Remove any leftover install directory contents (e.g. files that were
  ; locked while pi processes were still shutting down).
  RMDir /r /REBOOTOK "$INSTDIR"
!macroend
