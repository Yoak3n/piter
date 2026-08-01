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
  ; Note: resources are placed directly in $INSTDIR (next to the exe), so pi
  ; lives at $INSTDIR\pi (not $INSTDIR\resources\pi).
  DetailPrint "Removing pi runtime directory..."
  RMDir /r "$INSTDIR\pi"
!macroend

; Runs after Tauri removed the installed files, registry keys and shortcuts.
!macro NSIS_HOOK_POSTUNINSTALL
  ; App data lives in $APPDATA\${BUNDLEID} (com.yoa.piter), which Tauri's
  ; built-in "delete app data" checkbox already removes when selected.
  ; Here we only clean up any leftover install directory contents (e.g. files
  ; that were locked while pi processes were still shutting down).
  RMDir /r /REBOOTOK "$INSTDIR"
!macroend
