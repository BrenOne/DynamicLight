; Notify Explorer that the executable (including its icon resource) was replaced.
; SHCNE_UPDATEITEM + SHCNF_PATHW | SHCNF_FLUSHNOWAIT. No cache deletion/restart.
; https://learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shchangenotify
;
; DynamicLight runs elevated through two scheduled tasks (see autostart_windows.rs):
; "DynamicLight Launch" lets shortcuts open the elevated app, "DynamicLight" starts it
; at sign-in. The installer offers them once; silent installs and updates that find
; the tasks already registered do not ask again.
!macro NSIS_HOOK_POSTINSTALL
  System::Call 'shell32::SHChangeNotify(i 0x00002000, i 0x3005, w "$INSTDIR\${MAINBINARYNAME}.exe", p 0)'
  IfSilent dl_tasks_done
  nsExec::Exec 'schtasks /Query /TN "DynamicLight Launch"'
  Pop $0
  StrCmp $0 "0" dl_tasks_done
  MessageBox MB_YESNO|MB_ICONQUESTION "Start DynamicLight with Windows?$\r$\n$\r$\nDynamicLight runs with administrator rights so Caps Lock protection works in every window. Windows asks for permission once.$\r$\n$\r$\nYou can change this later in Settings > Launch at login." IDNO dl_tasks_nologin
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --setup-tasks login'
    Goto dl_tasks_done
  dl_tasks_nologin:
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --setup-tasks'
  dl_tasks_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Interactive uninstall only: updates run the old uninstaller silently and must
  ; keep the tasks. Asks for permission only when a task exists.
  IfSilent dl_remove_done
  IfFileExists "$INSTDIR\${MAINBINARYNAME}.exe" 0 dl_remove_done
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --remove-tasks'
  dl_remove_done:
!macroend
