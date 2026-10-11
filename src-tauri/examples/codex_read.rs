//! One read-only quota request. Does not start the Tauri GUI or persist the response.

#[cfg(unix)]
fn main() {
    use knowyourlimits_lib::codex;
    use std::path::PathBuf;
    use tokio::signal::unix::{signal, SignalKind};

    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() > 1 || args.first().is_some_and(|a| a == "--help" || a == "-h") {
        eprintln!("Usage: cargo run --example codex_read -- [trusted-absolute-codex-path]");
        std::process::exit(if args.len() > 1 { 64 } else { 0 });
    }
    let executable = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("codex"));

    let exit_code = tauri::async_runtime::block_on(async {
        // Install shutdown handlers before starting the isolated child group.
        let Ok(mut terminate) = signal(SignalKind::terminate()) else {
            eprintln!("Unable to install shutdown handler.");
            return 1;
        };
        let outcome = tokio::select! {
            result = codex::read(&executable) => Some(result),
            _ = tokio::signal::ctrl_c() => None,
            _ = terminate.recv() => None,
        };
        // select drops the cancelled read future before returning this branch.
        // Its owner guard kills the child group; no stdout/stderr is exposed here.
        match outcome {
            Some(Ok(value)) => match serde_json::to_string(&value) {
                Ok(json) => {
                    println!("{json}");
                    0
                }
                Err(_) => {
                    eprintln!("Unable to serialize quota snapshot.");
                    1
                }
            },
            Some(Err(error)) => {
                eprintln!("{error}");
                1
            }
            None => {
                eprintln!("Quota read cancelled.");
                130
            }
        }
    });
    std::process::exit(exit_code);
}

#[cfg(not(unix))]
fn main() {
    eprintln!("This Codex spike requires a Unix platform.");
    std::process::exit(1);
}
