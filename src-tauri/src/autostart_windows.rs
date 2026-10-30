//! Elevated start-up through Task Scheduler.
//!
//! An elevated process is needed so the lock-key guard (a low-level keyboard
//! hook) also sees key presses while an administrator window is in front; Windows
//! hides those from non-elevated hooks. Two tasks run DynamicLight with the
//! highest available rights without a UAC prompt:
//! - `DynamicLight` starts it in the tray at sign-in (Launch at login);
//! - `DynamicLight Launch` has no trigger: a normally started copy runs it and
//!   exits, so shortcuts and the installer also open the elevated app.
//!
//! Creating or removing tasks asks for consent once.
use std::{os::windows::process::CommandExt, process::Command};
use windows::{
    core::{w, HSTRING, PCWSTR},
    Win32::{
        Foundation::CloseHandle,
        System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE},
        UI::{
            Shell::{
                IsUserAnAdmin, ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SEE_MASK_NO_CONSOLE,
                SHELLEXECUTEINFOW,
            },
            WindowsAndMessaging::{ChangeWindowMessageFilter, MSGFLT_ADD, SW_HIDE, WM_COPYDATA},
        },
    },
};

const TASK: &str = "DynamicLight";
const LAUNCH: &str = "DynamicLight Launch";
/// Argument of processes started by the launch task; they never relaunch.
const FROM_TASK: &str = "--from-task";
const NO_WINDOW: u32 = 0x0800_0000;

fn exists(task: &str) -> bool {
    Command::new("schtasks")
        .args(["/Query", "/TN", task])
        .creation_flags(NO_WINDOW)
        .output()
        .is_ok_and(|o| o.status.success())
}

pub fn enabled() -> bool {
    exists(TASK)
}

/// Login task plus the launch task, under one UAC prompt.
pub fn enable() -> Result<(), String> {
    register(true)
}

/// Removes only the login task; the app keeps opening elevated.
pub fn disable() -> Result<(), String> {
    elevated(&format!("/c schtasks /Delete /F /TN \"{TASK}\""))
}

fn current_user() -> Result<String, String> {
    match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(domain), Ok(name)) if !domain.is_empty() => Ok(format!("{domain}\\{name}")),
        (_, Ok(name)) => Ok(name),
        _ => Err("Cannot determine the current user".into()),
    }
}

fn register(login: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_string_lossy();
    let user = current_user()?;
    let mut tasks = vec![(LAUNCH, task_xml(&exe, &user, false, FROM_TASK))];
    if login {
        tasks.push((TASK, task_xml(&exe, &user, true, "--autostart")));
    }
    let mut commands = Vec::new();
    let mut files = Vec::new();
    for (i, (name, xml)) in tasks.iter().enumerate() {
        let path = std::env::temp_dir().join(format!("dynamiclight-task-{i}.xml"));
        // Task Scheduler expects UTF-16 XML.
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        commands.push(format!(
            "schtasks /Create /F /TN \"{name}\" /XML \"{}\"",
            path.display()
        ));
        files.push(path);
    }
    let result = elevated(&format!("/c {}", commands.join(" && ")));
    for path in files {
        let _ = std::fs::remove_file(path);
    }
    result
}

fn remove_all() -> Result<(), String> {
    let commands: Vec<_> = [TASK, LAUNCH]
        .into_iter()
        .filter(|task| exists(task))
        .map(|task| format!("schtasks /Delete /F /TN \"{task}\""))
        .collect();
    if commands.is_empty() {
        return Ok(());
    }
    elevated(&format!("/c {}", commands.join(" & ")))
}

/// Installer/uninstaller entry points (`--setup-tasks [login]`, `--remove-tasks`)
/// and self-elevation. Returns an exit code when this process should stop.
pub fn startup() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    if has("--setup-tasks") {
        return Some(i32::from(register(has("login")).is_err()));
    }
    if has("--remove-tasks") {
        return Some(i32::from(remove_all().is_err()));
    }
    // A normally started copy hands over to the elevated launch task, which
    // shows the window (or passes it to an already running instance).
    if !is_elevated() && !has(FROM_TASK) && !has("--autostart") && exists(LAUNCH) {
        let ran = Command::new("schtasks")
            .args(["/Run", "/TN", LAUNCH])
            .creation_flags(NO_WINDOW)
            .status()
            .is_ok_and(|s| s.success());
        if ran {
            return Some(0);
        }
    }
    None
}

/// Whether this process has administrator rights (cached).
pub fn is_elevated() -> bool {
    static ELEVATED: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| unsafe { IsUserAnAdmin() }.as_bool());
    *ELEVATED
}

/// Registers the launch task (one UAC prompt) and restarts through it, so the
/// app comes back elevated. The restart waits ~4 s for this copy to exit.
pub fn elevate_and_restart() -> Result<(), String> {
    if !exists(LAUNCH) {
        register(false)?;
    }
    Command::new("cmd.exe")
        .args([
            "/c",
            "ping -n 5 127.0.0.1 >nul & schtasks /Run /TN \"DynamicLight Launch\"",
        ])
        .creation_flags(NO_WINDOW)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// The elevated app may receive hand-overs from a normally started copy
/// (single-instance uses WM_COPYDATA, which UIPI blocks from lower-integrity
/// senders by default).
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

fn task_xml(exe: &str, user: &str, at_logon: bool, arguments: &str) -> String {
    let (exe, user) = (escape(exe), escape(user));
    let triggers = if at_logon {
        format!(
            "<Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId></LogonTrigger></Triggers>"
        )
    } else {
        String::new()
    };
    // The launch task runs again while its previous run (the open app) is alive.
    let instances = if at_logon { "IgnoreNew" } else { "Parallel" };
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Start DynamicLight with administrator rights</Description></RegistrationInfo>
  {triggers}
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>{instances}</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>5</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>{exe}</Command><Arguments>{arguments}</Arguments></Exec></Actions>
</Task>
"#
    )
}

/// Runs `cmd.exe <arguments>` (schtasks commands) with a UAC prompt and waits.
fn elevated(arguments: &str) -> Result<(), String> {
    let parameters = HSTRING::from(arguments);
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NO_CONSOLE;
        info.lpVerb = w!("runas");
        info.lpFile = w!("cmd.exe");
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
        let xml = super::task_xml(r#"C:\A&B\<x>.exe"#, "PC\\user", true, "--autostart");
        assert!(xml.contains(r"<Command>C:\A&amp;B\&lt;x&gt;.exe</Command>"));
        assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(xml.contains("<Arguments>--autostart</Arguments>"));
        assert!(xml.contains("<LogonTrigger>"));
        assert!(xml.contains("IgnoreNew"));
        let launch = super::task_xml(r"C:\d.exe", "PC\\user", false, super::FROM_TASK);
        assert!(!launch.contains("<Triggers>"));
        assert!(launch.contains("Parallel"));
        assert!(launch.contains("<Arguments>--from-task</Arguments>"));
    }
}
