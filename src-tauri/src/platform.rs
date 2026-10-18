#[cfg(not(target_os = "linux"))]
use crate::engine::Handle;

#[cfg(target_os = "windows")]
pub fn set_taskbar_icon(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{HINSTANCE, HWND, LPARAM, WPARAM},
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{
                LoadImageW, SendMessageW, ICON_BIG, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED,
                WM_GETICON, WM_SETICON,
            },
        },
    };
    let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
    // Tauri's window icon sets ICON_SMALL only. Windows uses ICON_BIG for
    // the taskbar and Alt+Tab. Resource 32512 is emitted by tauri-build.
    // LR_SHARED keeps the embedded icon valid until the process exits.
    unsafe {
        let module = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let icon = LoadImageW(
            Some(HINSTANCE(module.0)),
            PCWSTR(32512_usize as *const u16),
            IMAGE_ICON,
            0,
            0,
            LR_SHARED | LR_DEFAULTSIZE,
        )
        .map_err(|e| format!("Could not load taskbar icon: {e}"))?;
        SendMessageW(
            hwnd,
            WM_SETICON,
            Some(WPARAM(ICON_BIG as usize)),
            Some(LPARAM(icon.0 as isize)),
        );
        if SendMessageW(hwnd, WM_GETICON, Some(WPARAM(ICON_BIG as usize)), None).0
            != icon.0 as isize
        {
            return Err("Windows did not retain the taskbar icon".into());
        }
    }
    Ok(())
}

pub struct FrameClock {
    active: bool,
}
#[cfg(target_os = "windows")]
#[link(name = "winmm")]
extern "system" {
    fn timeBeginPeriod(period: u32) -> u32;
    fn timeEndPeriod(period: u32) -> u32;
}
impl FrameClock {
    pub fn new() -> Self {
        Self { active: false }
    }
    pub fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        #[cfg(target_os = "windows")]
        unsafe {
            if active {
                timeBeginPeriod(1);
            } else {
                timeEndPeriod(1);
            }
        }
        self.active = active;
    }
}
impl Drop for FrameClock {
    fn drop(&mut self) {
        self.set_active(false);
    }
}

#[cfg(target_os = "windows")]
pub fn watch_power(handle: Handle) -> Result<(), String> {
    crate::lifecycle_windows::watch(handle.clone())?;
    use std::ffi::c_void;
    #[repr(C)]
    struct Subscription {
        callback: unsafe extern "system" fn(*mut c_void, u32, *mut c_void) -> u32,
        context: *mut c_void,
    }
    #[link(name = "PowrProf")]
    extern "system" {
        fn PowerRegisterSuspendResumeNotification(
            flags: u32,
            recipient: *mut c_void,
            registration: *mut *mut c_void,
        ) -> u32;
    }
    unsafe extern "system" fn callback(context: *mut c_void, kind: u32, _: *mut c_void) -> u32 {
        // The OS owns this registration for the application's lifetime. The boxed
        // sender remains allocated until process exit, so callbacks cannot dangle.
        if kind == 4 || kind == 18 {
            (*(context as *const Handle)).power_event(kind == 4);
        }
        0
    }
    let context = Box::into_raw(Box::new(handle));
    let mut subscription = Subscription {
        callback,
        context: context.cast(),
    };
    let mut registration = std::ptr::null_mut();
    let code = unsafe {
        PowerRegisterSuspendResumeNotification(
            2,
            (&mut subscription as *mut Subscription).cast(),
            &mut registration,
        )
    };
    if code != 0 {
        unsafe {
            drop(Box::from_raw(context));
        }
        return Err(format!("Could not register sleep notifications ({code})"));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use crate::audio::Feed;
    use std::ffi::{c_char, c_void, CStr};
    extern "C" {
        fn dl_watch_power(callback: extern "C" fn(*mut c_void, u32, bool), context: *mut c_void);
        fn dl_capture_allowed(microphone: bool) -> bool;
        fn dl_microphone_permission() -> bool;
        fn dl_audio_start(
            callback: extern "C" fn(*mut c_void, *const f32, usize),
            error: extern "C" fn(*mut c_void),
            context: *mut c_void,
            message: *mut c_char,
            length: usize,
        ) -> *mut c_void;
        fn dl_audio_stop(stream: *mut c_void) -> bool;
    }
    pub fn watch_power(handle: Handle) -> Result<(), String> {
        extern "C" fn callback(ctx: *mut c_void, kind: u32, active: bool) {
            unsafe {
                use crate::lifecycle::SystemEvent;
                let event = match kind {
                    0 => SystemEvent::Sleep(active),
                    1 => SystemEvent::DisplayOff(active),
                    2 => SystemEvent::Locked(active),
                    3 => SystemEvent::SourcesChanged,
                    _ => SystemEvent::Ready,
                };
                (*(ctx as *const Handle)).system_event(event, kind == 0 && active);
            }
        }
        unsafe {
            dl_watch_power(callback, Box::into_raw(Box::new(handle)).cast());
        }
        Ok(())
    }
    pub fn check_capture_permission(settings: &crate::settings::Settings) -> Result<(), String> {
        let mic = matches!(settings.mode.as_str(), "music" | "blend")
            && settings.audio_source == "microphone";
        if unsafe {
            dl_capture_allowed(mic) && (settings.mode != "blend" || dl_capture_allowed(false))
        } {
            Ok(())
        } else {
            Err("Capture permission is missing. Allow access in System Settings → Privacy & Security, then click Retry.".into())
        }
    }
    pub fn microphone_permission() -> Result<(), String> {
        if unsafe { dl_microphone_permission() } {
            Ok(())
        } else {
            Err(
                "Allow DynamicLight microphone access in System Settings → Privacy & Security."
                    .into(),
            )
        }
    }
    pub struct MacAudio {
        stream: *mut c_void,
        context: *mut Feed,
    }
    impl MacAudio {
        pub fn start(feed: Feed) -> Result<Self, String> {
            extern "C" fn samples(ctx: *mut c_void, data: *const f32, len: usize) {
                // Native bridge delivers mono f32 samples valid for this callback.
                unsafe {
                    if let Ok(mut s) = (*(ctx as *const Feed)).try_lock() {
                        s.push(std::slice::from_raw_parts(data, len).iter().copied());
                    }
                }
            }
            extern "C" fn error(ctx: *mut c_void) {
                unsafe {
                    if let Ok(mut s) = (*(ctx as *const Feed)).lock() {
                        s.error =
                            Some("System audio capture stopped. Start capture to retry.".into());
                    }
                }
            }
            let context = Box::into_raw(Box::new(feed));
            let mut message = [0i8; 512];
            let stream = unsafe {
                dl_audio_start(
                    samples,
                    error,
                    context.cast(),
                    message.as_mut_ptr(),
                    message.len(),
                )
            };
            if stream.is_null() {
                unsafe {
                    drop(Box::from_raw(context));
                }
                return Err(unsafe { CStr::from_ptr(message.as_ptr()) }
                    .to_string_lossy()
                    .into_owned());
            }
            Ok(Self { stream, context })
        }
    }
    impl Drop for MacAudio {
        fn drop(&mut self) {
            // On a failed stop retain the callback context until process exit instead
            // of risking a late OS callback accessing freed memory.
            if unsafe { dl_audio_stop(self.stream) } {
                unsafe {
                    drop(Box::from_raw(self.context));
                }
            }
        }
    }
}
#[cfg(target_os = "linux")]
pub use crate::linux::watch_power;
#[cfg(target_os = "macos")]
pub use mac::*;
#[cfg(not(target_os = "macos"))]
pub fn check_capture_permission(_: &crate::settings::Settings) -> Result<(), String> {
    Ok(())
}
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn watch_power(_: Handle) -> Result<(), String> {
    Err("This operating system is not supported".into())
}
