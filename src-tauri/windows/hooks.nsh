; hooks.nsh
; Canopy's additions to Tauri's NSIS installer (bundle.windows.nsis.installerHooks).

; on uninstall, hand the .londopy double-click to another installed Londopy app
; (or remove it if none is left) before the exe goes away. skipped when the
; uninstall is the first half of an update, since the new copy registers itself
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    nsExec::Exec '"$INSTDIR\Canopy.exe" --londopy-unregister'
    Pop $0
  ${EndIf}
!macroend
