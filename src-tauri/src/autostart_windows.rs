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
//! Creating or removing tasks asks for consent once: DynamicLight restarts
//! itself elevated and talks to Task Scheduler through its COM API. No
//! `cmd.exe`/`schtasks.exe` command lines and no temporary task files: that
//! pattern (shell + delay + highest-rights task) is what antivirus heuristics
//! report as persistence malware.

use windows::{
    core::{w, BSTR, HSTRING, PCWSTR},
    Win32::{
        Foundation::CloseHandle,
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
                COINIT_MULTITHREADED,
            },
            TaskScheduler::{
                ITaskFolder, ITaskService, TaskScheduler, TASK_CREATE_OR_UPDATE,
                TASK_LOGON_INTERACTIVE_TOKEN,
            },
            Threading::{
                GetExitCodeProcess, OpenProcess, WaitForSingleObject, INFINITE,
                PROCESS_SYNCHRONIZE,
            },
            Variant::VARIANT,
        },
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
/// `--relaunch-after <pid>`: wait for that process to exit, then run the launch task.
const RELAUNCH: &str = "--relaunch-after";
/// `--user <DOMAIN\name>`: whose tasks an elevated helper registers (the
/// administrator who approved UAC may be a different account).
const USER: &str = "--user";

/// Runs `f` on the Task Scheduler root folder in a thread of its own, so COM is
/// initialised the same way whatever thread (UI, async command) calls it.
fn scheduler<T: Send>(
    f: impl FnOnce(&ITaskFolder) -> windows::core::Result<T> + Send,
) -> Result<T, String> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| unsafe {
                let init = CoInitializeEx(None, COINIT_MULTITHREADED);
                let result = (|| {
                    let service: ITaskService =
                        CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)?;
                    let none = VARIANT::default();
                    service.Connect(&none, &none, &none, &none)?;
                    f(&service.GetFolder(&BSTR::from("\\"))?)
                })();
                if init.is_ok() {
                    CoUninitialize();
                }
                result
            })
            .join()
            .map_err(|_| "Task Scheduler thread failed".to_string())?
            .map_err(|e| format!("Task Scheduler: {}", e.message()))
    })
}

fn exists(task: &str) -> bool {
    scheduler(|folder| unsafe { folder.GetTask(&BSTR::from(task)) }.map(|_| ())).is_ok()
}

fn run(task: &str) -> Result<(), String> {
    scheduler(|folder| unsafe {
        folder
            .GetTask(&BSTR::from(task))?
            .Run(&VARIANT::default())
            .map(|_| ())
    })
}

pub fn enabled() -> bool {
    exists(TASK)
}

pub fn enable() -> Result<(), String> {
    register(true, None)
}

pub fn disable() -> Result<(), String> {
    if !is_elevated() {
        return as_admin("--remove-login-task");
    }
    delete(&[TASK])
}

fn current_user() -> Result<String, String> {
    match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(domain), Ok(name)) if !domain.is_empty() => Ok(format!("{domain}\\{name}")),
        (_, Ok(name)) => Ok(name),
        _ => Err("Cannot determine the current user".into()),
    }
}

fn register(login: bool, user: Option<&str>) -> Result<(), String> {
    let user = match user {
        Some(user) => user.to_string(),
        None => current_user()?,
    };
    if !is_elevated() {
        let login = if login { " login" } else { "" };
        return as_admin(&format!("--setup-tasks{login} {USER} \"{user}\""));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_string_lossy();
    let mut tasks = vec![(LAUNCH, task_xml(&exe, &user, false, FROM_TASK))];
    if login {
        tasks.push((TASK, task_xml(&exe, &user, true, "--autostart")));
    }
    scheduler(|folder| {
        for (name, xml) in &tasks {
            let none = VARIANT::default();
            unsafe {
                folder.RegisterTask(
                    &BSTR::from(*name),
                    &BSTR::from(xml.as_str()),
                    TASK_CREATE_OR_UPDATE.0,
                    &none,
                    &none,
                    TASK_LOGON_INTERACTIVE_TOKEN,
                    &none,
                )?;
            }
        }
        Ok(())
    })
}

fn delete(tasks: &[&str]) -> Result<(), String> {
    let tasks: Vec<_> = tasks.iter().filter(|task| exists(task)).collect();
    scheduler(|folder| {
        for task in tasks {
            unsafe { folder.DeleteTask(&BSTR::from(*task), 0)? };
        }
        Ok(())
    })
}

fn remove_all() -> Result<(), String> {
    if ![TASK, LAUNCH].iter().any(|task| exists(task)) {
        return Ok(());
    }
    if !is_elevated() {
        return as_admin("--remove-tasks");
    }
    delete(&[TASK, LAUNCH])
}

/// Waits (at most 10 s) for process `pid` to exit.
fn wait_exit(pid: u32) {
    unsafe {
        if let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            WaitForSingleObject(process, 10_000);
            let _ = CloseHandle(process);
        }
    }
}

/// Handles the task-maintenance and hand-over starts. `Some(code)`: exit now.
pub fn startup() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
    };
    if let Some(pid) = value(RELAUNCH).and_then(|pid| pid.parse().ok()) {
        wait_exit(pid);
        return Some(i32::from(run(LAUNCH).is_err()));
    }
    if has("--setup-tasks") {
        return Some(i32::from(
            register(has("login"), value(USER).map(String::as_str)).is_err(),
        ));
    }
    if has("--remove-tasks") {
        return Some(i32::from(remove_all().is_err()));
    }
    if has("--remove-login-task") {
        return Some(i32::from(disable().is_err()));
    }
    // A normal start (shortcut, installer) hands over to the elevated launch task.
    if !is_elevated() && !has(FROM_TASK) && !has("--autostart") && run(LAUNCH).is_ok() {
        return Some(0);
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
/// app comes back elevated. A helper copy of DynamicLight waits for this one to
/// exit (otherwise single-instance would hand the new start back to it) and
/// then runs the task; it is a GUI process, so no console window appears.
pub fn elevate_and_restart() -> Result<(), String> {
    // Always (re)create it: a task that exists but is disabled would not run,
    // and the app would close without coming back.
    register(false, None)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe)
        .args([RELAUNCH, &std::process::id().to_string()])
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

/// Runs this executable with `arguments` and a UAC prompt, and waits for it.
fn as_admin(arguments: &str) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let file = HSTRING::from(exe.as_os_str());
    let parameters = HSTRING::from(arguments);
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NO_CONSOLE;
        info.lpVerb = w!("runas");
        info.lpFile = PCWSTR(file.as_ptr());
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
