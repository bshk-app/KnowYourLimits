//! Opt-in standalone Claude quota smoke; no Tauri GUI or raw output persistence.
use std::ffi::OsString;
fn opted_in(args: &[OsString]) -> bool {
    args.len() == 2 && args[0] == "--live" && std::path::Path::new(&args[1]).is_absolute()
}

#[cfg(unix)]
fn main() {
    use knowyourlimits_lib::claude;
    use tokio::signal::unix::{signal, SignalKind};
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !opted_in(&args) {
        eprintln!("Usage: cargo run --locked --example claude_read -- --live <trusted-absolute-claude-path>\nExplicit opt-in: CLI-owned auth, policy, network/cache/activity operations. Requires Claude Code 2.1.296; no model query or PTY fallback.");
        std::process::exit(
            if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
                0
            } else {
                64
            },
        );
    }
    let code = tauri::async_runtime::block_on(async {
        let Ok(mut terminate) = signal(SignalKind::terminate()) else {
            eprintln!("Unable to install shutdown handler.");
            return 1;
        };
        let Ok(mut interrupt) = signal(SignalKind::interrupt()) else {
            eprintln!("Unable to install shutdown handler.");
            return 1;
        };
        let (cancel, receiver) = tauri::async_runtime::channel(1);
        let executable = std::path::Path::new(&args[1]);
        let path = std::env::join_paths([
            executable.parent().unwrap_or(std::path::Path::new("/")),
            std::path::Path::new("/opt/homebrew/bin"),
            std::path::Path::new("/usr/local/bin"),
            std::path::Path::new("/usr/bin"),
            std::path::Path::new("/bin"),
            std::path::Path::new("/usr/sbin"),
            std::path::Path::new("/sbin"),
        ])
        .expect("known absolute bin locations");
        let read = claude::read(executable, &path, receiver);
        tokio::pin!(read);
        let outcome = tokio::select! {
            result = &mut read => Some(result),
            _ = terminate.recv() => None,
            _ = interrupt.recv() => None,
        };
        let Some(outcome) = outcome else {
            let _ = cancel.try_send(());
            let _ = read.await;
            eprintln!("Quota read cancelled and cleaned up.");
            return 130;
        };
        match outcome {
            Ok(snapshot) => match serde_json::to_string(&snapshot) {
                Ok(json) => {
                    println!("{json}");
                    0
                }
                Err(_) => {
                    eprintln!("Unable to serialize quota snapshot.");
                    1
                }
            },
            Err(error) => {
                eprintln!("{error}");
                1
            }
        }
    });
    std::process::exit(code);
}
#[cfg(not(unix))]
fn main() {
    let _ = opted_in(&std::env::args_os().skip(1).collect::<Vec<_>>());
    eprintln!("This Claude spike requires Unix.");
    std::process::exit(1);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_explicit_opt_in_and_exactly_one_absolute_executable() {
        let absolute = std::env::current_exe().unwrap().into_os_string();
        assert!(!opted_in(&[]));
        assert!(!opted_in(&[absolute.clone()]));
        assert!(!opted_in(&["--live".into(), "claude".into()]));
        assert!(!opted_in(&[
            "--live".into(),
            absolute.clone(),
            "extra".into()
        ]));
        assert!(opted_in(&["--live".into(), absolute]));
    }
}
