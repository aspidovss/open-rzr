; open-rzr: additions to the Tauri NSIS installer (bundle > windows > nsis > installerHooks).
;
; Install:   the licence page shows the user agreement (packaging/windows/agreement.txt); setup
;            cannot go on without accepting it, so after install the accepted edition is written
;            to %APPDATA%\open-rzr\agreement.txt — the program then does not ask again.
;            A local file in the program's own config folder, no registry.
; Uninstall: Tauri already closes the running program (also from the tray) and offers
;            "Delete the application data" (unchecked by default). Here: remove the autostart
;            entry the program made, and with that checkbox also delete %APPDATA%\open-rzr
;            (settings, profile library, backups, logs). An update (/UPDATE) keeps everything.

; must equal AGREEMENT_EDITION in app/src/commands/settings.rs (checked by tools/uitest/legal-check.js)
!define OPENRZR_AGREEMENT 2

!macro NSIS_HOOK_POSTINSTALL
  CreateDirectory "$APPDATA\open-rzr"
  ClearErrors
  FileOpen $0 "$APPDATA\open-rzr\agreement.txt" w
  ${IfNot} ${Errors}
    FileWrite $0 "${OPENRZR_AGREEMENT}"
    FileClose $0
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ; autostart made by the program itself (Settings > Start with Windows)
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "open-rzr"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    ${If} $DeleteAppDataCheckboxState = 1
      RMDir /r "$APPDATA\open-rzr"
    ${Else}
      DetailPrint "Settings, profiles and logs are kept in $APPDATA\open-rzr"
    ${EndIf}
    DetailPrint "Settings written to the mouse stay in the mouse."
  ${EndIf}
!macroend
