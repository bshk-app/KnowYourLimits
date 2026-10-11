//! On-demand Unix Codex reader. Never retains server identity or diagnostic text.
//! Requires a live Tokio runtime through cancellation cleanup; owner SIGKILL is
//! not recoverable. CLI startup metadata/cache/network activity is not ruled out.
use crate::model::{now_ms, ProviderId, ProviderSnapshot, ProviderStatus, QuotaWindow};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{collections::HashMap, fmt, path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStderr, ChildStdin, ChildStdout},
    time::timeout,
};

const RPC_TIMEOUT: Duration = Duration::from_secs(15);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_FRAME: usize = 64 * 1024;
const MAX_STDOUT: usize = 128 * 1024;
const MAX_STDERR: usize = 256 * 1024;
const MAX_FRAMES: usize = 100;
const MAX_JS_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexRead {
    pub snapshot: crate::model::ProviderSnapshot,
    pub ordinary_usage_allowed: Option<bool>,
}

/// Static categories only. RPC codes are the sole retained server error field.
#[derive(Debug, PartialEq, Eq)]
pub enum CodexError {
    Io,
    Protocol,
    InvalidResponse,
    Rpc(i32),
    ResourceLimit,
    Timeout,
    Cancelled,
    Cleanup,
}

impl fmt::Display for CodexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Io => "Codex I/O failed",
            Self::Protocol => "Invalid Codex protocol",
            Self::InvalidResponse => "Invalid Codex quota response",
            Self::Rpc(_) => "Codex RPC failed",
            Self::ResourceLimit => "Codex resource limit exceeded",
            Self::Timeout => "Codex request timed out",
            Self::Cancelled => "Codex request cancelled",
            Self::Cleanup => "Codex cleanup failed",
        })
    }
}

impl std::error::Error for CodexError {}

// Decode only the selected bucket's quota fields. The legacy object is required,
// but a present map is authoritative even when it contains no Codex bucket.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitsResponse {
    rate_limits: Map<String, Value>,
    rate_limits_by_limit_id: Option<HashMap<String, Value>>,
    ordinary_usage_allowed: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bucket {
    limit_id: Option<String>,
    primary: Option<Map<String, Value>>,
    secondary: Option<Map<String, Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Window {
    used_percent: i32,
    window_duration_mins: Option<i64>,
    resets_at: Option<i64>,
}

fn quota_window(slot: &str, window: Window) -> Result<QuotaWindow, CodexError> {
    if !(0..=100).contains(&window.used_percent) {
        return Err(CodexError::InvalidResponse);
    }
    let duration_mins = match window.window_duration_mins {
        None => 0,
        Some(n) if n > 0 => u32::try_from(n).map_err(|_| CodexError::InvalidResponse)?,
        Some(_) => return Err(CodexError::InvalidResponse),
    };
    let label = match duration_mins {
        0 => "Неизвестная длительность".into(),
        10080 => "Неделя".into(),
        n if n % 60 == 0 => format!("{} часов", n / 60),
        n => format!("{n} минут"),
    };
    let resets_at = window
        .resets_at
        .map(|seconds| {
            u64::try_from(seconds)
                .ok()
                .and_then(|seconds| seconds.checked_mul(1000))
                .filter(|&ms| ms <= MAX_JS_INTEGER)
                .ok_or(CodexError::InvalidResponse)
        })
        .transpose()?;
    Ok(QuotaWindow {
        id: format!("codex:{slot}"),
        label,
        used_percent: f64::from(window.used_percent),
        resets_at,
        duration_mins,
        // Existing P0 MIN-counter uses this flag, not the server's slot name.
        primary: true,
    })
}

fn normalize(value: Value, now: u64) -> Result<CodexRead, CodexError> {
    let response: RateLimitsResponse =
        serde_json::from_value(value).map_err(|_| CodexError::InvalidResponse)?;
    let selected = match response.rate_limits_by_limit_id {
        Some(mut map) => map.remove("codex"),
        None => Some(Value::Object(response.rate_limits)),
    };
    let mut windows = Vec::new();
    let ordinary_usage_allowed = response.ordinary_usage_allowed;
    if let Some(value) = selected {
        if !value.is_object() {
            return Err(CodexError::InvalidResponse);
        }
        let bucket: Bucket =
            serde_json::from_value(value).map_err(|_| CodexError::InvalidResponse)?;
        if bucket.limit_id.as_deref().is_none_or(|id| id == "codex") {
            for (slot, window) in [("primary", bucket.primary), ("secondary", bucket.secondary)] {
                if let Some(window) = window {
                    let window: Window = serde_json::from_value(Value::Object(window))
                        .map_err(|_| CodexError::InvalidResponse)?;
                    windows.push(quota_window(slot, window)?);
                }
            }
        }
    }
    Ok(CodexRead {
        snapshot: ProviderSnapshot {
            id: ProviderId::Codex,
            label: "Codex".into(),
            status: if windows.is_empty() || ordinary_usage_allowed == Some(false) {
                ProviderStatus::Unavailable
            } else {
                ProviderStatus::Current
            },
            fetched_at: Some(now),
            windows,
        },
        ordinary_usage_allowed,
    })
}

/// Exactly one initialize → initialized → quota exchange. The RPC deadline is
/// 15s, including writes. Normal completion waits up to another 2s for cleanup;
/// cleanup failure returns `Cleanup` and schedules reaping on the live runtime.
/// Cancellation kills synchronously in Drop, then schedules (not awaits) reap.
pub async fn read(executable: &Path) -> Result<CodexRead, CodexError> {
    read_inner(executable, None, None).await
}

/// Managed cleanup retains ownership until wait confirms exit, without a cutoff.
/// OS wait errors retry with backoff; cancellation also takes this owned path.
/// The caller must await this future, never abort a normal mode/exit transition.
pub async fn read_managed(
    executable: &Path,
    child_path: &std::ffi::OsStr,
    cancellation: tauri::async_runtime::Receiver<()>,
) -> Result<CodexRead, CodexError> {
    read_inner(executable, Some(child_path), Some(cancellation)).await
}

async fn read_inner(
    executable: &Path,
    child_path: Option<&std::ffi::OsStr>,
    mut cancellation: Option<tauri::async_runtime::Receiver<()>>,
) -> Result<CodexRead, CodexError> {
    let managed = cancellation.is_some();
    // A stop requested before scheduling must not spawn even a short-lived child.
    if cancellation.as_mut().is_some_and(|rx| {
        !matches!(
            rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        )
    }) {
        return Err(CodexError::Cancelled);
    }
    let mut command = crate::process::command(executable);
    if let Some(path) = child_path {
        command.env("PATH", path);
    }
    // Preserve CODEX_HOME/auth and layered MCP settings. Plugins are disabled by
    // their pinned feature guards, not by an unproven mcp_servers override.
    for setting in [
        "analytics.enabled=false",
        "otel.exporter=\"none\"",
        "otel.trace_exporter=\"none\"",
        "otel.metrics_exporter=\"none\"",
        "features.plugins=false",
        "features.remote_plugin=false",
        "features.hooks=false",
        "features.apps=false",
    ] {
        command.args(["-c", setting]);
    }
    command
        .args(["app-server", "--listen", "stdio://"])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    crate::process::isolate(&mut command);
    let child = command.spawn().map_err(|_| CodexError::Io)?;
    #[cfg(windows)]
    let mut child = child;
    // A just-spawned Unix child's positive PID fits pid_t. No wait/try_wait has
    // run: it remains reserved until we explicitly reap it AFTER the group kill.
    #[cfg(windows)]
    let job = match crate::process::ProcessJob::attach(&child) {
        Ok(job) => job,
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err(CodexError::Io);
        }
    };
    #[cfg(unix)]
    let pid = child.id().expect("newly spawned child has a PID") as libc::pid_t;
    let mut owner = Owner {
        child: Some(child),
        #[cfg(unix)]
        pid: Some(pid),
        #[cfg(windows)]
        job,
        killed: false,
    };
    let result = {
        let exchange = timeout(RPC_TIMEOUT, async {
            let child = owner.child.as_mut().ok_or(CodexError::Io)?;
            let stdin = child.stdin.take().ok_or(CodexError::Io)?;
            let stdout = child.stdout.take().ok_or(CodexError::Io)?;
            let stderr = child.stderr.take().ok_or(CodexError::Io)?;
            let rpc = exchange(stdin, stdout);
            tokio::pin!(rpc);
            // Scoped drain future, not a spawned task: completion/cancellation drops
            // it and its pipe, so no stderr task can outlive the owning call.
            tokio::select! {
                biased;
                drained = discard_stderr(stderr) => { drained?; rpc.await }
                result = &mut rpc => result,
            }
        });
        tokio::pin!(exchange);
        tokio::select! {
            biased;
            _ = async {
                match cancellation.as_mut() {
                    Some(receiver) => { receiver.recv().await; }
                    None => std::future::pending::<()>().await,
                }
            } => Err(CodexError::Cancelled),
            result = &mut exchange => result.unwrap_or(Err(CodexError::Timeout)),
        }
    };
    owner.cleanup(managed).await?;
    result
}

struct Owner {
    child: Option<Child>,
    #[cfg(unix)]
    pid: Option<libc::pid_t>,
    #[cfg(windows)]
    job: crate::process::ProcessJob,
    killed: bool,
}

impl Owner {
    fn kill_group(&mut self) {
        if !self.killed {
            #[cfg(windows)]
            self.job.terminate();
            #[cfg(unix)]
            if let Some(pid) = self.pid {
                // SAFETY: this is only our own positive, unreaped child PID;
                // negative PID addresses its isolated group, never other groups.
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
            }
            self.killed = true;
        }
    }

    async fn cleanup(&mut self, managed: bool) -> Result<(), CodexError> {
        self.kill_group();
        let child = self.child.as_mut().ok_or(CodexError::Cleanup)?;
        wait_for_exit(child, managed).await?;
        // PID reservation ends here. Drop must NEVER signal it again.
        #[cfg(unix)]
        {
            self.pid = None;
        }
        self.child = None;
        Ok(())
    }
}

trait ExitWait {
    fn wait_for_exit(&mut self) -> impl std::future::Future<Output = std::io::Result<()>> + Send;
}
impl ExitWait for Child {
    async fn wait_for_exit(&mut self) -> std::io::Result<()> {
        self.wait().await.map(|_| ())
    }
}
async fn wait_for_exit(child: &mut impl ExitWait, managed: bool) -> Result<(), CodexError> {
    if managed {
        // Keep the same child and owning job until wait confirms exit, even if
        // the OS is slow or temporarily fails. Never hand cleanup to Drop.
        loop {
            if child.wait_for_exit().await.is_ok() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    match timeout(CLEANUP_TIMEOUT, child.wait_for_exit()).await {
        Ok(Ok(())) => Ok(()),
        _ => Err(CodexError::Cleanup),
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.kill_group();
        if let Some(mut child) = self.child.take() {
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = child.wait().await;
                });
            }
            // Without a live runtime, Child's kill_on_drop remains a backup;
            // async reaping requires the caller to keep its runtime alive.
        }
        #[cfg(unix)]
        {
            self.pid = None;
        }
    }
}

async fn discard_stderr(mut stderr: ChildStderr) -> Result<(), CodexError> {
    let mut buffer = [0; 4096];
    let mut total = 0;
    loop {
        let remaining = MAX_STDERR - total;
        let capacity = remaining.min(buffer.len());
        let n = stderr
            .read(&mut buffer[..capacity])
            .await
            .map_err(|_| CodexError::Io)?;
        if n == 0 {
            return Ok(());
        }
        total += n;
        if total == MAX_STDERR {
            return Err(CodexError::ResourceLimit);
        }
    }
}

// Fixed read buffer plus a bounded frame; no read_line/read_until allocation.
struct Frames {
    stdout: ChildStdout,
    buffer: [u8; 4096],
    start: usize,
    end: usize,
    bytes: usize,
    count: usize,
}

impl Frames {
    async fn next(&mut self) -> Result<Value, CodexError> {
        let mut frame = Vec::with_capacity(4096);
        loop {
            if self.start == self.end {
                let capacity = (MAX_STDOUT - self.bytes).min(self.buffer.len());
                if capacity == 0 {
                    return Err(CodexError::ResourceLimit);
                }
                self.end = self
                    .stdout
                    .read(&mut self.buffer[..capacity])
                    .await
                    .map_err(|_| CodexError::Io)?;
                self.start = 0;
                if self.end == 0 {
                    return Err(CodexError::Protocol);
                }
                self.bytes += self.end;
            }
            let remaining = &self.buffer[self.start..self.end];
            let newline = remaining.iter().position(|&byte| byte == b'\n');
            let n = newline.unwrap_or(remaining.len());
            if frame.len() + n > MAX_FRAME {
                return Err(CodexError::ResourceLimit);
            }
            frame.extend_from_slice(&remaining[..n]);
            self.start += n;
            if newline.is_some() {
                self.start += 1;
                self.count += 1;
                if self.count > MAX_FRAMES {
                    return Err(CodexError::ResourceLimit);
                }
                return serde_json::from_slice(&frame).map_err(|_| CodexError::Protocol);
            }
        }
    }

    async fn response(&mut self, expected_id: i64) -> Result<Value, CodexError> {
        loop {
            let mut value = self.next().await?;
            let object = value.as_object().ok_or(CodexError::Protocol)?;
            if let Some(method) = object.get("method") {
                if !method.is_string() || object.contains_key("id") {
                    // Server requests are unsupported; never send a fourth RPC.
                    return Err(CodexError::Protocol);
                }
                continue;
            }
            let id = object
                .get("id")
                .and_then(Value::as_i64)
                .ok_or(CodexError::Protocol)?;
            if object.contains_key("result") == object.contains_key("error") {
                return Err(CodexError::Protocol);
            }
            if id != expected_id {
                continue;
            }
            if let Some(error) = object.get("error") {
                let code = error
                    .get("code")
                    .and_then(Value::as_i64)
                    .and_then(|code| i32::try_from(code).ok())
                    .ok_or(CodexError::Protocol)?;
                return Err(CodexError::Rpc(code));
            }
            // Move only result; metadata and errors never enter a returned DTO.
            return Ok(value["result"].take());
        }
    }
}

async fn packet(stdin: &mut ChildStdin, value: Value) -> Result<(), CodexError> {
    let mut bytes = serde_json::to_vec(&value).map_err(|_| CodexError::Protocol)?;
    bytes.push(b'\n');
    stdin.write_all(&bytes).await.map_err(|_| CodexError::Io)?;
    stdin.flush().await.map_err(|_| CodexError::Io)
}

async fn exchange(mut stdin: ChildStdin, stdout: ChildStdout) -> Result<CodexRead, CodexError> {
    let mut frames = Frames {
        stdout,
        buffer: [0; 4096],
        start: 0,
        end: 0,
        bytes: 0,
        count: 0,
    };
    packet(
        &mut stdin,
        json!({"id":1,"method":"initialize","params":{
            "clientInfo":{"name":"knowyourlimits","version":env!("CARGO_PKG_VERSION")}
        }}),
    )
    .await?;
    let initialization = frames.response(1).await?;
    if !initialization.is_object() {
        return Err(CodexError::Protocol);
    }
    drop(initialization);
    packet(&mut stdin, json!({"method":"initialized"})).await?;
    packet(
        &mut stdin,
        json!({"id":2,"method":"account/rateLimits/read"}),
    )
    .await?;
    let result = frames.response(2).await?;
    normalize(result, now_ms())
}

#[cfg(all(test, unix))]
pub(crate) mod tests;
