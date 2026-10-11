//! Windows protocol and cancellation checks; only synthetic CLI fixtures run.
use crate::{claude, codex, model::ProviderStatus, process};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, BufReader};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str, source: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("kyl-test-{}-{unique}", std::process::id()));
        let entry = if name == "codex" {
            "node_modules/@openai/codex/bin/codex.js"
        } else {
            "node_modules/@anthropic-ai/claude-code/cli.js"
        };
        std::fs::create_dir_all(directory.join(entry).parent().unwrap()).unwrap();
        std::fs::write(directory.join(entry), source).unwrap();
        // The wrapper is deliberately invalid: the reader must invoke JS directly.
        std::fs::write(directory.join(format!("{name}.cmd")), "exit /b 99").unwrap();
        Self(directory)
    }
    fn executable(&self, name: &str) -> PathBuf {
        self.0.join(format!("{name}.cmd"))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn npm_codex_reads_normalized_quota_without_executing_the_wrapper() {
    let fake = Fixture::new(
        "codex",
        r#"
const readline = require('node:readline');
const input = readline.createInterface({ input: process.stdin });
input.on('line', line => {
  const request = JSON.parse(line);
  if (request.method === 'initialize') {
    console.log(JSON.stringify({id:request.id,result:{userAgent:'synthetic'}}));
  } else if (request.method === 'account/rateLimits/read') {
    console.log(JSON.stringify({id:request.id,result:{rateLimits:{primary:{usedPercent:42,windowDurationMins:300,resetsAt:1800000100}},ordinaryUsageAllowed:true}}));
  }
});
setInterval(() => {}, 1000);
"#,
    );
    let (_cancel, receiver) = tauri::async_runtime::channel(1);
    let result = codex::read_managed(
        &fake.executable("codex"),
        &std::env::var_os("PATH").unwrap(),
        receiver,
    )
    .await
    .unwrap();
    assert_eq!(result.snapshot.status, ProviderStatus::Current);
    assert_eq!(result.snapshot.windows[0].used_percent, 42.0);
}

#[tokio::test]
async fn npm_claude_reads_quota_and_handles_windows_process_exit() {
    let fake = Fixture::new(
        "claude",
        r#"
const args = process.argv.slice(2);
if (args.includes('--version')) { console.log('2.1.296 (Claude Code)'); }
else if (args.includes('status')) { console.log(JSON.stringify({loggedIn:true,apiProvider:'firstParty',authMethod:'claude.ai'})); }
else {
  const readline = require('node:readline');
  readline.createInterface({input:process.stdin}).on('line', line => {
    const request = JSON.parse(line);
    console.log(JSON.stringify({type:'control_response',response:{request_id:request.request_id,subtype:'success',response:{session:{total_cost_usd:0,model_usage:{}},rate_limits:{limits:[{kind:'session',group:'session',percent:25,resets_at:'2026-12-01T00:00:00Z'}]}}}}));
  });
  setInterval(() => {}, 1000);
}
"#,
    );
    let executable = std::fs::canonicalize(fake.executable("claude")).unwrap();
    let version = process::command(&executable)
        .arg("--version")
        .output()
        .await
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "2.1.296 (Claude Code)",
        "{}",
        String::from_utf8_lossy(&version.stderr)
    );
    let (_cancel, receiver) = tauri::async_runtime::channel(1);
    let result = claude::read(
        &fake.executable("claude"),
        &std::env::var_os("PATH").unwrap(),
        receiver,
    )
    .await
    .unwrap();
    assert_eq!(result.status, ProviderStatus::Current);
    assert_eq!(result.windows[0].used_percent, 25.0);
}

#[tokio::test]
async fn cancellation_waits_for_windows_cli_exit() {
    let fake = Fixture::new("codex", "setInterval(() => {}, 1000);");
    let (cancel, receiver) = tauri::async_runtime::channel(1);
    let executable = fake.executable("codex");
    let path = std::env::var_os("PATH").unwrap();
    let task = tokio::spawn(async move { codex::read_managed(&executable, &path, receiver).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.send(()).await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err(),
        codex::CodexError::Cancelled
    );
}

#[tokio::test]
async fn closing_job_terminates_cli_descendants() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    let executable = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut command = process::command(&executable);
    command.args(["-NoProfile", "-Command", "$child = Start-Process powershell.exe -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 120' -PassThru; Write-Output $child.Id; Start-Sleep -Seconds 120"])
        .stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    process::isolate(&mut command);
    let mut child = command.spawn().unwrap();
    let job = process::ProcessJob::attach(&child).unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(15), output.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    let pid = line.trim().parse::<u32>().unwrap();
    // SAFETY: open a borrowed descendant PID, then own its handle until the wait.
    let descendant = unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        assert!(!handle.is_null());
        OwnedHandle::from_raw_handle(handle)
    };
    drop(job);
    tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap();
    // SAFETY: a valid owned process handle is held throughout this bounded wait.
    assert_eq!(
        unsafe { WaitForSingleObject(descendant.as_raw_handle(), 5000) },
        0
    );
}
