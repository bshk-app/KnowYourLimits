//! Explicit opt-in smoke using the exact native coordinator and discovery policy.
use std::ffi::OsString;

fn opted_in(args: &[OsString]) -> bool {
    let args = args
        .strip_suffix(&[OsString::from("--persist")])
        .unwrap_or(args);
    (args.len() == 1 && args[0] == "--live")
        || (args.len() == 2 && args[0] == "--live" && args[1] == "claude")
}

#[cfg(unix)]
fn main() {
    use knowyourlimits_lib::{
        live::Coordinator,
        model::{AppState, ProviderId, Settings},
    };
    use tokio::signal::unix::{signal, SignalKind};

    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !opted_in(&args) {
        eprintln!("Usage: cargo run --locked --example live_read -- --live [claude] [--persist]\nExplicit opt-in: trusts installed CLI; CLI-owned auth/network/cache side effects. One read-only quota request, no inference. Never starts login. --persist loads/saves app-owned quota cache and cooldown; run while the app is closed.");
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
        let persist = args.last().is_some_and(|arg| arg == "--persist");
        let coordinator = Coordinator::new(AppState::disconnected(Settings::default()), |_| {});
        let coordinator = if persist {
            let Some(home) = std::env::var_os("HOME") else {
                return 1;
            };
            coordinator.with_persistence(
                std::path::PathBuf::from(home)
                    .join("Library/Application Support/dev.knowyourlimits.desktop"),
            )
        } else {
            coordinator
        };
        let provider = if args.get(1).is_some_and(|arg| arg == "claude") {
            ProviderId::Claude
        } else {
            ProviderId::Codex
        };
        if coordinator.read_quota(provider).await.is_err() {
            return 1;
        }
        let ready = tokio::select! {
            state = coordinator.wait_for_idle() => Some(state),
            _ = tokio::signal::ctrl_c() => None,
            _ = terminate.recv() => None,
        };
        // Never drop/abort the owned reader when interrupting the wait.
        coordinator.shutdown().await;
        match ready {
            Some(state) => {
                if let Ok(json) = serde_json::to_string(&state) {
                    println!("{json}");
                    i32::from(state.notice.is_some())
                } else {
                    eprintln!("Unable to serialize normalized state.");
                    1
                }
            }
            None => {
                eprintln!("Quota read cancelled and cleaned up.");
                130
            }
        }
    });
    std::process::exit(code);
}

#[cfg(not(unix))]
fn main() {
    let _ = opted_in(&std::env::args_os().skip(1).collect::<Vec<_>>());
    eprintln!("LIVE Codex requires a Unix platform.");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smoke_requires_explicit_opt_in_and_accepts_no_paths_or_extra_arguments() {
        assert!(!opted_in(&[]));
        assert!(!opted_in(&["/arbitrary/codex".into()]));
        assert!(!opted_in(&["--live".into(), "/arbitrary/codex".into()]));
        assert!(opted_in(&["--live".into()]));
        assert!(opted_in(&["--live".into(), "claude".into()]));
        assert!(opted_in(&["--live".into(), "--persist".into()]));
        assert!(opted_in(&[
            "--live".into(),
            "claude".into(),
            "--persist".into()
        ]));
        assert!(!opted_in(&["--persist".into()]));
    }
}
