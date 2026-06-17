; Force app data deletion on every uninstall (no opt-out via checkbox)
!macro NSIS_HOOK_PREUNINSTALL
  StrCpy $DeleteAppDataCheckboxState 1
!macroend
