//! Launch at login through Task Scheduler with the highest available rights.
//!
//! An elevated process is needed so the lock-key guard (a low-level keyboard
//! hook) also sees key presses while an administrator window is in front; Windows
//! hides those from non-elevated hooks. A scheduled task starts elevated at logon
//! without a UAC prompt; creating or removing it asks for consent once.
use std::{os::windows::process::CommandExt, process::Command};
use windows::{
    core::{w, HSTRING, PCWSTR},
    Win32::{
        Foundation::CloseHandle,
        System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE},
        UI::{
            Shell::{
                ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SEE_MASK_NO_CONSOLE, SHELLEXECUTEINFOW,
            },
            WindowsAndMessaging::{ChangeWindowMessageFilter, MSGFLT_ADD, SW_HIDE, WM_COPYDATA},
        },
    },
};

const TASK: &str = "DynamicLight";
const NO_WINDOW: u32 = 0x0800_0000;

pub fn enabled() -> bool {
    Command::new("schtasks")
        .args(["/Query", "/TN", TASK])
        .creation_flags(NO_WINDOW)
        .output()
        .is_ok_and(|o| o.status.success())
}

pub fn enable() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(domain), Ok(name)) if !domain.is_empty() => format!("{domain}\\{name}"),
        (_, Ok(name)) => name,
        _ => return Err("Cannot determine the current user".into()),
    };
    let xml = task_xml(&exe.to_string_lossy(), &user);
    let path = std::env::temp_dir().join("dynamiclight-login-task.xml");
    // Task Scheduler expects UTF-16 XML.
    let mut bytes = vec![0xff, 0xfe];
    bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let result = elevated(&format!(
        "/Create /F /TN \"{TASK}\" /XML \"{}\"",
        path.display()
    ));
    let _ = std::fs::remove_file(&path);
    result
}

pub fn disable() -> Result<(), String> {
    elevated(&format!("/Delete /F /TN \"{TASK}\""))
}

/// The task starts DynamicLight elevated; let a normally started second copy
/// still hand over to it (single-instance uses WM_COPYDATA, which UIPI blocks
/// from lower-integrity senders by default).
pub fn allow_relaunch_from_unelevated() {
    unsafe {
        let _ = ChangeWindowMessageFilter(WM_COPYDATA, MSGFLT_ADD);
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn task_xml(exe: &str, user: &str) -> String {
    let (exe, user) = (escape(exe), escape(user));
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Start DynamicLight at sign-in</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>5</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>{exe}</Command><Arguments>--autostart</Arguments></Exec></Actions>
</Task>
"#
    )
}

/// Runs schtasks.exe with a UAC prompt and waits for it.
fn elevated(arguments: &str) -> Result<(), String> {
    let parameters = HSTRING::from(arguments);
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NO_CONSOLE;
        info.lpVerb = w!("runas");
        info.lpFile = w!("schtasks.exe");
        info.lpParameters = PCWSTR(parameters.as_ptr());
        info.nShow = SW_HIDE.0;
        ShellExecuteExW(&mut info)
            .map_err(|_| "Administrator permission was not granted".to_string())?;
        if info.hProcess.is_invalid() {
            return Err("Task Scheduler did not start".into());
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1u32;
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = CloseHandle(info.hProcess);
        if code == 0 {
            Ok(())
        } else {
            Err(format!(
                "Task Scheduler could not update the login task (code {code})"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn task_xml_escapes_paths_and_users() {
        let xml = super::task_xml(r#"C:\A&B\<x>.exe"#, "PC\\user");
        assert!(xml.contains(r"<Command>C:\A&amp;B\&lt;x&gt;.exe</Command>"));
        assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(xml.contains("<Arguments>--autostart</Arguments>"));
    }
}
