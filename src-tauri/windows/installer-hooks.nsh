; Notify Explorer that the executable (including its icon resource) was replaced.
; SHCNE_UPDATEITEM + SHCNF_PATHW | SHCNF_FLUSHNOWAIT. No cache deletion/restart.
; https://learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shchangenotify
;
; Start with Windows is the per-user Run key, the same value the app's Launch at
; login writes (tauri-plugin-autostart: name = product name, --autostart). The
; installer asks once per user, without UAC; silent installs and updates never ask.
; Administrator rights (Task Scheduler tasks, see autostart_windows.rs) are an
; option in the app; users who already have the tasks keep them and are not asked.

!macro NSIS_HOOK_POSTINSTALL
  System::Call 'shell32::SHChangeNotify(i 0x00002000, i 0x3005, w "$INSTDIR\${MAINBINARYNAME}.exe", p 0)'
  IfSilent dl_startup_done
  ReadRegDWORD $0 HKCU "Software\${PRODUCTNAME}" "StartupAsked"
  StrCmp $0 "1" dl_startup_done
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
  StrCmp $0 "" 0 dl_startup_asked
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --has-tasks' $0
  StrCmp $0 "0" dl_startup_asked
  MessageBox MB_YESNO|MB_ICONQUESTION "Start DynamicLight with Windows?" IDNO dl_startup_asked
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}" '"$INSTDIR\${MAINBINARYNAME}.exe" --autostart'
  dl_startup_asked:
    WriteRegDWORD HKCU "Software\${PRODUCTNAME}" "StartupAsked" 1
  dl_startup_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Interactive uninstall only: updates run the old uninstaller silently and must
  ; keep start-up as it is. Removing tasks asks for permission only when one exists.
  IfSilent dl_remove_done
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
  DeleteRegValue HKCU "Software\${PRODUCTNAME}" "StartupAsked"
  DeleteRegKey /ifempty HKCU "Software\${PRODUCTNAME}"
  IfFileExists "$INSTDIR\${MAINBINARYNAME}.exe" 0 dl_remove_done
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --remove-tasks'
  dl_remove_done:
!macroend
