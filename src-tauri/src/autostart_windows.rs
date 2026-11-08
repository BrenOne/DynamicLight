//! Elevated start-up through Task Scheduler ("Run with administrator rights").
//!
//! The option is off by default: then Launch at login is the per-user Run key
//! and DynamicLight runs without administrator rights (up to 30 FPS).
//! An elevated process is needed so the lock-key guard (a low-level keyboard
//! hook) also sees key presses while an administrator window is in front; Windows
//! hides those from non-elevated hooks. Two tasks run DynamicLight with the
//! highest available rights without a UAC prompt:
//! - `DynamicLight (user)` starts it in the tray at sign-in (Launch at login);
//! - `DynamicLight Launch (user)` has no trigger: a normally started copy runs it
//!   and exits, so shortcuts and the installer also open the elevated app.
//!
//! Creating or removing tasks asks for consent once: DynamicLight restarts
//! itself elevated and talks to Task Scheduler through its COM API. No
//! `cmd.exe`/`schtasks.exe` command lines and no temporary task files: that
//! pattern (shell + delay + highest-rights task) is what antivirus heuristics
//! report as persistence malware.

use windows::{
    core::{w, BSTR, HSTRING, PCWSTR},
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{
            GetTokenInformation, TokenElevation, TokenElevationType, TokenElevationTypeLimited,
            TOKEN_ELEVATION, TOKEN_ELEVATION_TYPE, TOKEN_INFORMATION_CLASS, TOKEN_QUERY,
        },
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
                GetCurrentProcess, GetExitCodeProcess, OpenProcess, OpenProcessToken,
                WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE,
            },
            Variant::VARIANT,
        },
        UI::{
            Shell::{
                ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SEE_MASK_NO_CONSOLE, SHELLEXECUTEINFOW,
            },
            WindowsAndMessaging::{
                ChangeWindowMessageFilter, FindWindowExW, FindWindowW, MSGFLT_ADD, SW_HIDE,
                WM_COPYDATA,
            },
        },
    },
};

/// Names of the tasks before they were made per user. They belong to whoever
/// registered them last; they are moved to the per-user names when found.
const LEGACY_TASK: &str = "DynamicLight";
const LEGACY_LAUNCH: &str = "DynamicLight Launch";
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

/// The account name without its domain (`PC\name` -> `name`).
fn account(user: &str) -> &str {
    user.rsplit('\\').next().unwrap_or(user)
}

/// Task names are global to the computer, so every Windows user gets their own
/// pair; otherwise a second user's tasks would replace the first one's.
fn task_name(user: &str) -> String {
    format!("DynamicLight ({})", account(user))
}
fn launch_name(user: &str) -> String {
    format!("DynamicLight Launch ({})", account(user))
}

/// Whether an old shared-name task exists and runs as `user`.
fn legacy_owned(task: &str, user: &str) -> bool {
    scheduler(|folder| unsafe {
        let principal = folder
            .GetTask(&BSTR::from(task))?
            .Definition()?
            .Principal()?;
        let mut owner = BSTR::new();
        principal.UserId(&mut owner)?;
        Ok(owner.to_string())
    })
    .is_ok_and(|owner| account(&owner).eq_ignore_ascii_case(account(user)))
}

/// Elevated: replaces this user's old shared-name tasks by per-user ones.
fn migrate(user: &str) {
    let login = legacy_owned(LEGACY_TASK, user);
    let launch = legacy_owned(LEGACY_LAUNCH, user);
    if (login || launch) && register(login, Some(user)).is_ok() {
        let old: Vec<_> = [(login, LEGACY_TASK), (launch, LEGACY_LAUNCH)]
            .into_iter()
            .filter_map(|(owned, task)| owned.then_some(task))
            .collect();
        let _ = delete(&old);
    }
}

pub fn enabled() -> bool {
    current_user().is_ok_and(|user| exists(&task_name(&user)) || legacy_owned(LEGACY_TASK, &user))
}

/// Whether "Run with administrator rights" is on: this user has the launch task.
pub fn admin_mode() -> bool {
    current_user()
        .is_ok_and(|user| exists(&launch_name(&user)) || legacy_owned(LEGACY_LAUNCH, &user))
}

/// Login task (administrator rights on).
pub fn enable() -> Result<(), String> {
    register(true, None)
}

pub fn disable() -> Result<(), String> {
    remove(false, None)
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
    let mut tasks = vec![(launch_name(&user), task_xml(&exe, &user, false, FROM_TASK))];
    if login {
        tasks.push((task_name(&user), task_xml(&exe, &user, true, "--autostart")));
    }
    scheduler(|folder| {
        for (name, xml) in &tasks {
            let none = VARIANT::default();
            unsafe {
                folder.RegisterTask(
                    &BSTR::from(name.as_str()),
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

/// Removes this user's login task (`all`: also the launch task), including
/// their old shared-name ones; other users' tasks are left alone.
fn remove(all: bool, user: Option<&str>) -> Result<(), String> {
    let user = match user {
        Some(user) => user.to_string(),
        None => current_user()?,
    };
    let mut tasks = vec![(task_name(&user), LEGACY_TASK)];
    if all {
        tasks.push((launch_name(&user), LEGACY_LAUNCH));
    }
    let mut found = Vec::new();
    for (task, legacy) in &tasks {
        if exists(task) {
            found.push(task.as_str());
        }
        if legacy_owned(legacy, &user) {
            found.push(legacy);
        }
    }
    if found.is_empty() {
        return Ok(());
    }
    if !is_elevated() {
        let flag = if all {
            "--remove-tasks"
        } else {
            "--remove-login-task"
        };
        return as_admin(&format!("{flag} {USER} \"{user}\""));
    }
    delete(&found)
}

/// Runs this user's launch task (or their old shared-name one).
fn launch(user: &str) -> Result<(), String> {
    run(&launch_name(user)).or_else(|e| {
        if legacy_owned(LEGACY_LAUNCH, user) {
            run(LEGACY_LAUNCH)
        } else {
            Err(e)
        }
    })
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
    let user = value(USER).map(String::as_str);
    let code = |result: Result<(), String>| Some(i32::from(result.is_err()));
    if has("--setup-tasks") {
        return code(register(has("login"), user));
    }
    if has("--remove-tasks") {
        return code(remove(true, user));
    }
    if has("--remove-login-task") {
        return code(remove(false, user));
    }
    let Ok(me) = current_user() else {
        return None;
    };
    // Installer: exit code 0 when this user already has the launch task.
    if has("--has-tasks") {
        return Some(i32::from(!admin_mode()));
    }
    if let Some(pid) = value(RELAUNCH).and_then(|pid| pid.parse().ok()) {
        wait_exit(pid);
        return code(launch(&me));
    }
    if is_elevated() {
        migrate(&me);
    } else if !has(FROM_TASK) && !has("--autostart") && launch(&me).is_ok() {
        // A normal start (shortcut, installer) hands over to the elevated launch task.
        return Some(0);
    }
    None
}

fn token_info<T: Default>(class: TOKEN_INFORMATION_CLASS) -> Option<T> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
        let mut value = T::default();
        let mut size = 0;
        let result = GetTokenInformation(
            token,
            class,
            Some((&mut value as *mut T).cast()),
            std::mem::size_of::<T>() as u32,
            &mut size,
        );
        let _ = CloseHandle(token);
        result.ok().map(|_| value)
    }
}

/// Whether this process has administrator rights (cached).
pub fn is_elevated() -> bool {
    static ELEVATED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
        token_info::<TOKEN_ELEVATION>(TokenElevation).is_some_and(|t| t.TokenIsElevated != 0)
    });
    *ELEVATED
}

/// Whether this account can get administrator rights: elevated already, or an
/// administrator under UAC. A standard user's task would run without them.
pub fn can_elevate() -> bool {
    is_elevated()
        || token_info::<TOKEN_ELEVATION_TYPE>(TokenElevationType)
            .is_some_and(|t| t == TokenElevationTypeLimited)
}

/// Turns administrator rights on: registers the launch task (and the login task
/// when `login`) with one UAC prompt. Returns true when this copy must exit: a
/// helper copy of DynamicLight waits for it (otherwise single-instance would
/// hand the new start back to it) and then runs the task, so the app comes back
/// elevated; it is a GUI process, so no console window appears.
pub fn enable_admin(login: bool) -> Result<bool, String> {
    // Always (re)create it: a task that exists but is disabled would not run,
    // and the app would close without coming back.
    register(login, None)?;
    if is_elevated() {
        return Ok(false);
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe)
        .args([RELAUNCH, &std::process::id().to_string()])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// Turns administrator rights off: removes this user's tasks (one UAC prompt
/// unless elevated). A running elevated copy keeps its rights until it exits.
pub fn disable_admin() -> Result<(), String> {
    remove(true, None)
}

/// At sign-in the task starts before Explorer has created the notification area,
/// and a tray icon added earlier never shows. A start with `--autostart` waits
/// (at most a minute) until the taskbar's tray exists.
pub fn wait_for_shell() {
    if !std::env::args().any(|arg| arg == "--autostart") {
        return;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        let ready = unsafe {
            FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null())
                .and_then(|taskbar| {
                    FindWindowExW(Some(taskbar), None, w!("TrayNotifyWnd"), PCWSTR::null())
                })
                .is_ok()
        };
        if ready {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
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
    fn task_names_are_per_account() {
        assert_eq!(super::task_name(r"PC\Nikita"), "DynamicLight (Nikita)");
        assert_eq!(super::launch_name("Nikita"), "DynamicLight Launch (Nikita)");
        assert_ne!(super::launch_name(r"PC\a"), super::launch_name(r"PC\b"));
    }
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
