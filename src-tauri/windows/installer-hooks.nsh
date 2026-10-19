; Notify Explorer that the executable (including its icon resource) was replaced.
; SHCNE_UPDATEITEM + SHCNF_PATHW | SHCNF_FLUSHNOWAIT. No cache deletion/restart.
; https://learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shchangenotify
!macro NSIS_HOOK_POSTINSTALL
  System::Call 'shell32::SHChangeNotify(i 0x00002000, i 0x3005, w "$INSTDIR\${MAINBINARYNAME}.exe", p 0)'
!macroend
