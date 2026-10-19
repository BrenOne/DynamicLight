use dynamiclight_control::{
    http,
    session::{self, Connection},
    Command, Power, Response,
};
use std::path::PathBuf;

const HELP: &str = "DynamicLight CLI\nUsage: dynamiclight [--json] [--connection FILE] COMMAND\n  status\n  power on|off|toggle [--allow-capture]\n  brightness 0..100\n  scenes list\n  scenes apply ID [--allow-capture]\n  profiles list\n  profiles apply ID [--allow-capture]\n\nEnable Local automation in the desktop app's Settings first.\nCapture needs both the Settings permission and --allow-capture.\n--json prints the versioned API response; errors exit 1, usage errors exit 2.";
struct Options {
    command: Command,
    path: Option<PathBuf>,
    json: bool,
}
fn parse(args: Vec<String>) -> Result<Options, String> {
    let mut path = None;
    let mut json = false;
    let mut capture = false;
    let mut words = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" if !json => json = true,
            "--connection" if path.is_none() => {
                path = Some(PathBuf::from(
                    args.next().ok_or("--connection needs a file")?,
                ))
            }
            "--allow-capture" if !capture => capture = true,
            value if value.starts_with('-') => {
                return Err(format!("Unknown or repeated option: {value}"))
            }
            _ => words.push(arg),
        }
    }
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let command = match words.as_slice() {
        ["status"] if !capture => Command::Status {},
        ["power", state] => Command::Power {
            state: match *state {
                "on" => Power::On,
                "off" => Power::Off,
                "toggle" => Power::Toggle,
                _ => return Err("Power must be on, off or toggle".into()),
            },
            allow_capture: capture,
        },
        ["brightness", value] if !capture => Command::Brightness {
            value: value.parse().map_err(|_| "Brightness must be 0–100")?,
        },
        ["scenes", "list"] if !capture => Command::Scenes {},
        ["profiles", "list"] if !capture => Command::Profiles {},
        ["scenes", "apply", id] => Command::ApplyScene {
            id: (*id).into(),
            allow_capture: capture,
        },
        ["profiles", "apply", id] => Command::ApplyProfile {
            id: (*id).into(),
            allow_capture: capture,
        },
        _ => return Err("Invalid command; use --help".into()),
    };
    command.validate()?;
    Ok(Options {
        command,
        path,
        json,
    })
}
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        println!("{HELP}");
        return std::process::ExitCode::SUCCESS;
    }
    if args == ["--version"] {
        println!("DynamicLight CLI {} (API 1)", env!("CARGO_PKG_VERSION"));
        return std::process::ExitCode::SUCCESS;
    }
    let options = match parse(args) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("{e}");
            return 2.into();
        }
    };
    let result = (|| {
        let path = options.path.map(Ok).unwrap_or_else(session::default_path)?;
        http::call(&Connection::read(&path)?, options.command)
    })();
    let response = result.unwrap_or_else(|e| Response::from_result(Err(e)));
    if options.json {
        println!("{}", serde_json::to_string(&response).unwrap());
    } else if let Some(data) = &response.data {
        println!("{}", serde_json::to_string_pretty(data).unwrap());
    }
    if !response.ok {
        eprintln!("{}", response.error.as_deref().unwrap_or("Command failed"));
        return 1.into();
    }
    std::process::ExitCode::SUCCESS
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }
    #[test]
    fn parses_commands_without_silent_capture_consent() {
        assert!(matches!(
            parse(args("power on")).unwrap().command,
            Command::Power {
                allow_capture: false,
                ..
            }
        ));
        assert!(matches!(
            parse(args("--json scenes apply cinema --allow-capture"))
                .unwrap()
                .command,
            Command::ApplyScene {
                allow_capture: true,
                ..
            }
        ));
        for s in [
            "brightness 101",
            "brightness -1",
            "status --allow-capture",
            "power maybe",
            "profiles apply ../file",
            "status extra",
            "status --json --json",
            "status --connection",
        ] {
            assert!(parse(args(s)).is_err(), "{s}");
        }
    }
}
