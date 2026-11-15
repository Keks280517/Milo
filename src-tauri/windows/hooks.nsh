; Milo startet ueber eine Windows-Aufgabe bei der Anmeldung (siehe lifecycle.rs).
; Beim echten Deinstallieren (nicht beim Update) wird sie mit entfernt.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    nsExec::Exec 'schtasks /delete /tn "Milo" /f'
  ${EndIf}
!macroend
