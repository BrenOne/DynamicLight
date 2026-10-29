use crate::{engine::Handle, lifecycle::SystemEvent};
use windows::{
    core::{w, GUID, PWSTR},
    Win32::{
        Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::{LibraryLoader::GetModuleHandleW, Power::*, RemoteDesktop::*},
        UI::WindowsAndMessaging::*,
    },
};

// Interactive applications must use the session display state, not the kernel/console GUID.
// https://learn.microsoft.com/en-us/windows/win32/power/power-setting-guids
const DISPLAY_STATE: GUID = GUID::from_u128(0x2b84c20e_ad23_4ddf_93db_05ffbd7efca5);

pub fn watch(handle: Handle) -> Result<(), String> {
    std::thread::Builder::new()
        .name("session-events".into())
        .spawn(move || {
            if let Err(e) = unsafe { message_loop(&handle) } {
                handle.system_event(
                    SystemEvent::Warning(format!("Display/lock events unavailable: {e}")),
                    false,
                );
                handle.system_event(SystemEvent::Ready, false);
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Handle;
    if let Some(handle) = pointer.as_ref() {
        match message {
            WM_WTSSESSION_CHANGE if wparam.0 == 7 || wparam.0 == 8 => {
                handle.system_event(SystemEvent::Locked(wparam.0 == 7), false);
            }
            WM_POWERBROADCAST if wparam.0 == PBT_POWERSETTINGCHANGE as usize && lparam.0 != 0 => {
                let setting = &*(lparam.0 as *const POWERBROADCAST_SETTING);
                if setting.PowerSetting == DISPLAY_STATE && setting.DataLength == 4 {
                    let value = std::ptr::read_unaligned(setting.Data.as_ptr().cast::<u32>());
                    // Dimmed (2) still counts as on. Only fully off (0) pauses lighting.
                    handle.system_event(SystemEvent::DisplayOff(value == 0), false);
                    handle.system_event(SystemEvent::Ready, false);
                }
            }
            WM_DISPLAYCHANGE => handle.system_event(SystemEvent::SourcesChanged, false),
            // Windows shutdown/restart/sign-out: turn the strip off before the process is
            // killed, otherwise USB power keeps the last frame lit.
            WM_QUERYENDSESSION => return LRESULT(1),
            WM_ENDSESSION if wparam.0 != 0 => {
                let _ = handle.request(crate::engine::Action::Shutdown);
            }
            _ => {}
        }
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

/// Low-level keyboard hook that only looks at Caps/Num/Scroll Lock key-downs, which
/// make Windows send an indicator report to the strip's keyboard interface.
unsafe extern "system" fn lock_keys(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN) && lparam.0 != 0 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        // VK_CAPITAL, VK_NUMLOCK, VK_SCROLL
        if matches!(info.vkCode, 0x14 | 0x90 | 0x91) {
            crate::device::hold_output(250);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe fn message_loop(handle: &Handle) -> windows::core::Result<()> {
    let instance = HINSTANCE(GetModuleHandleW(None)?.0);
    let class = w!("DynamicLightSessionEvents");
    let definition = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class,
        ..Default::default()
    };
    if RegisterClassW(&definition) == 0 {
        return Err(windows::core::Error::from_thread());
    }
    // Hidden top-level window receives broadcast display changes; a message-only window does not.
    // Handle lives on this thread's stack until the window is destroyed.
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        w!(""),
        WINDOW_STYLE::default(),
        0,
        0,
        0,
        0,
        None,
        None,
        Some(instance),
        Some((handle as *const Handle).cast()),
    )?;
    let session = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
    if let Err(e) = &session {
        handle.system_event(
            SystemEvent::Warning(format!("Lock notifications unavailable: {e}")),
            false,
        );
    }
    let registration = RegisterPowerSettingNotification(
        HANDLE(hwnd.0),
        &DISPLAY_STATE,
        DEVICE_NOTIFY_WINDOW_HANDLE,
    );
    if let Err(e) = &registration {
        handle.system_event(
            SystemEvent::Warning(format!("Display sleep notifications unavailable: {e}")),
            false,
        );
        handle.system_event(SystemEvent::Ready, false);
    }
    let mut buffer = PWSTR::null();
    let mut bytes = 0;
    if WTSQuerySessionInformationW(
        None,
        WTS_CURRENT_SESSION,
        WTSSessionInfoEx,
        &mut buffer,
        &mut bytes,
    )
    .is_ok()
    {
        if bytes as usize >= std::mem::size_of::<WTSINFOEXW>() {
            let info = &*buffer.0.cast::<WTSINFOEXW>();
            if info.Level == 1 {
                handle.system_event(
                    SystemEvent::Locked(
                        info.Data.WTSInfoExLevel1.SessionFlags == WTS_SESSIONSTATE_LOCK as i32,
                    ),
                    false,
                );
            }
        }
        WTSFreeMemory(buffer.0.cast());
    }
    let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(lock_keys), Some(instance), 0).ok();
    let mut message = MSG::default();
    while GetMessageW(&mut message, None, 0, 0).0 > 0 {
        let _ = TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    if let Some(hook) = hook {
        let _ = UnhookWindowsHookEx(hook);
    }
    if session.is_ok() {
        let _ = WTSUnRegisterSessionNotification(hwnd);
    }
    if let Ok(registration) = registration {
        let _ = UnregisterPowerSettingNotification(registration);
    }
    let _ = DestroyWindow(hwnd);
    Ok(())
}
