//! Pinned, opt-in official CLI quota read. No raw terminal/identity output escapes.
use crate::model::{now_ms, ProviderId, ProviderSnapshot, ProviderStatus, QuotaWindow};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fmt,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Mutex,
    time::Duration,
};
use tauri::async_runtime::Receiver;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStderr, ChildStdout},
    time::{timeout_at, Instant},
};

const MIN_VERSION: [u32; 3] = [2, 1, 296];
const CLI_CACHE_MS: u64 = 60_000;
const MAX_JS_INTEGER: u64 = 9_007_199_254_740_991;
const READ_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_FRAME: usize = 64 * 1024;
const MAX_STREAM: usize = 256 * 1024;
const MAX_FRAMES: usize = 64;

const ISOLATION_ARGS: &[&str] = &[
    "--safe-mode", "--print", "--output-format", "stream-json", "--verbose",
    "--no-session-persistence", "--tools", "", "--strict-mcp-config",
    "--mcp-config", "{\"mcpServers\":{}}", "--no-chrome", "--settings",
    "{\"disableAllHooks\":true,\"disableClaudeAiConnectors\":true,\"autoContinueAtUsageLimit\":false}",
];
const QUOTA_ARGS: &[&str] = &["--input-format", "stream-json", "--max-turns", "0"];
// One minimal model turn: its response headers carry the plan windows when the
// usage endpoint itself is rate limited.
const PROBE_ARGS: &[&str] = &[
    "--max-turns",
    "1",
    "--model",
    "haiku",
    "--system-prompt",
    "Reply with a single dot.",
    ".",
];
const REQUEST_ID: &str = "usage";

/// Usage endpoint 429 cooldowns per canonical CLI, like the CLI's per-bearer memory:
/// while one is active, a read skips the usage request and goes straight to the probe.
static ENDPOINT_COOLDOWN: Mutex<BTreeMap<PathBuf, Instant>> = Mutex::new(BTreeMap::new());

#[derive(Debug, PartialEq, Eq)]
pub enum ClaudeError {
    Io,
    UnsupportedVersion,
    NotAuthenticated,
    UnsupportedAccount,
    Protocol,
    InvalidResponse,
    Command,
    ResourceLimit,
    RateLimited { retry_after_secs: u32 },
    Timeout,
    Cancelled,
}
impl fmt::Display for ClaudeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Io => "Claude I/O failed",
            Self::UnsupportedVersion => {
                "Claude CLI version is unsupported (requires 2.1.296 or newer)"
            }
            Self::NotAuthenticated => "Claude Code is not signed in",
            Self::UnsupportedAccount => "Claude subscription authentication is required",
            Self::Protocol => "Invalid Claude quota protocol",
            Self::InvalidResponse => "Invalid Claude quota response",
            Self::Command => "Claude quota command failed",
            Self::RateLimited { .. } => "Claude quota endpoint rate limited",
            Self::ResourceLimit => "Claude resource limit exceeded",
            Self::Timeout => "Claude quota read timed out",
            Self::Cancelled => "Claude quota read cancelled",
        })
    }
}
impl std::error::Error for ClaudeError {}

fn normalize(report: Option<Value>, started_at: u64) -> Result<ProviderSnapshot, ClaudeError> {
    let mut windows = Vec::new();
    if let Some(report) = report.filter(|value| !value.is_null()) {
        let report = report.as_object().ok_or(ClaudeError::InvalidResponse)?;
        if let Some(limits) = report.get("rate_limits").filter(|value| !value.is_null()) {
            let limits = limits.as_object().ok_or(ClaudeError::InvalidResponse)?;
            if let Some(rows) = limits.get("limits").filter(|value| !value.is_null()) {
                let rows = rows.as_array().ok_or(ClaudeError::InvalidResponse)?;
                for row in rows {
                    let row = row.as_object().ok_or(ClaudeError::InvalidResponse)?;
                    let kind = row
                        .get("kind")
                        .and_then(Value::as_str)
                        .ok_or(ClaudeError::InvalidResponse)?;
                    let Some((_, group, _)) = window_meta(kind) else {
                        continue;
                    };
                    if row.get("group").and_then(Value::as_str) != Some(group)
                        || row.get("scope").is_some_and(|value| !value.is_null())
                        || windows
                            .iter()
                            .any(|window: &QuotaWindow| window.id == format!("claude:{kind}"))
                    {
                        return Err(ClaudeError::InvalidResponse);
                    }
                    let used_percent = row
                        .get("percent")
                        .and_then(Value::as_f64)
                        .filter(|value| value.is_finite() && (0.0..=100.0).contains(value))
                        .ok_or(ClaudeError::InvalidResponse)?;
                    let resets_at = match row.get("resets_at") {
                        Some(Value::Null) => None,
                        Some(Value::String(text)) => {
                            let millis = chrono::DateTime::parse_from_rfc3339(text)
                                .map_err(|_| ClaudeError::InvalidResponse)?
                                .timestamp_millis();
                            Some(
                                u64::try_from(millis)
                                    .ok()
                                    .filter(|value| *value <= MAX_JS_INTEGER)
                                    .ok_or(ClaudeError::InvalidResponse)?,
                            )
                        }
                        _ => return Err(ClaudeError::InvalidResponse),
                    };
                    windows.extend(quota_window(kind, used_percent, resets_at));
                }
            }
        }
    }
    Ok(snapshot(windows, started_at.saturating_sub(CLI_CACHE_MS)))
}

/// Global windows shown by the app: label, `limits` group and duration in minutes.
fn window_meta(kind: &str) -> Option<(&'static str, &'static str, u32)> {
    match kind {
        "session" => Some(("5 часов", "session", 300)),
        "weekly_all" => Some(("Неделя", "weekly", 10080)),
        _ => None,
    }
}
fn quota_window(kind: &str, used_percent: f64, resets_at: Option<u64>) -> Option<QuotaWindow> {
    let (label, _, duration_mins) = window_meta(kind)?;
    Some(QuotaWindow {
        id: format!("claude:{kind}"),
        label: label.into(),
        used_percent,
        resets_at,
        duration_mins,
        primary: true,
    })
}
fn snapshot(mut windows: Vec<QuotaWindow>, observed_at: u64) -> ProviderSnapshot {
    // Provider order is stable even when the CLI returns the weekly row first.
    windows.sort_by_key(|window| window.duration_mins);
    let current = !windows.is_empty();
    ProviderSnapshot {
        id: ProviderId::Claude,
        label: "Claude".into(),
        status: if current {
            ProviderStatus::Current
        } else {
            ProviderStatus::Unavailable
        },
        fetched_at: current.then_some(observed_at),
        windows,
    }
}

/// Plan windows from the probe's rate-limit headers, observed by this request.
fn header_windows(
    windows: Option<Value>,
    started_at: u64,
) -> Result<ProviderSnapshot, ClaudeError> {
    let mut found = Vec::new();
    if let Some(windows) = windows.filter(|value| !value.is_null()) {
        let windows = windows.as_object().ok_or(ClaudeError::InvalidResponse)?;
        for (key, kind) in [("five_hour", "session"), ("seven_day", "weekly_all")] {
            let Some(row) = windows.get(key).filter(|value| !value.is_null()) else {
                continue;
            };
            let utilization = row
                .get("utilization")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or(ClaudeError::InvalidResponse)?;
            let resets_at = match row.get("resetsAt") {
                None | Some(Value::Null) => None,
                Some(value) => Some(
                    value
                        .as_u64()
                        .filter(|secs| *secs <= MAX_JS_INTEGER / 1000)
                        .ok_or(ClaudeError::InvalidResponse)?
                        * 1000,
                ),
            };
            // Headers report a 0..1 fraction; overage may exceed 1.
            let used_percent = (utilization * 10_000.0).round().min(10_000.0) / 100.0;
            found.extend(quota_window(kind, used_percent, resets_at));
        }
    }
    Ok(snapshot(found, started_at))
}

/// The undocumented `/usage` format is verified from MIN_VERSION; later changes
/// fail strict response validation instead of being guessed.
fn supported_version(text: &str) -> bool {
    text.trim()
        .strip_suffix(" (Claude Code)")
        .and_then(|version| {
            version
                .split('.')
                .map(|part| part.parse::<u32>().ok())
                .collect::<Option<Vec<_>>>()
        })
        .is_some_and(|parts| parts.len() == 3 && parts.as_slice() >= MIN_VERSION.as_slice())
}

/// CLI-owned authentication and quota. No credential/config files are read by this app.
pub async fn read(
    executable: &Path,
    child_path: &OsStr,
    cancellation: Receiver<()>,
) -> Result<ProviderSnapshot, ClaudeError> {
    read_or_sign_in(executable, child_path, cancellation, false).await
}
pub async fn read_or_sign_in(
    executable: &Path,
    child_path: &OsStr,
    mut cancellation: Receiver<()>,
    sign_in: bool,
) -> Result<ProviderSnapshot, ClaudeError> {
    if !executable.is_absolute() {
        return Err(ClaudeError::Io);
    }
    let executable = std::fs::canonicalize(executable).map_err(|_| ClaudeError::Io)?;
    let started_at = now_ms();
    let deadline = Instant::now() + READ_TIMEOUT;
    match run(
        &executable,
        child_path,
        Stage::Version,
        started_at,
        deadline,
        &mut cancellation,
    )
    .await?
    {
        Output::Version(text) if supported_version(&text) => {}
        _ => return Err(ClaudeError::UnsupportedVersion),
    }
    if sign_in {
        run(
            &executable,
            child_path,
            Stage::Login,
            started_at,
            Instant::now() + Duration::from_secs(180),
            &mut cancellation,
        )
        .await?;
    }
    let started_at = now_ms();
    let deadline = if sign_in {
        Instant::now() + READ_TIMEOUT
    } else {
        deadline
    };
    match run(
        &executable,
        child_path,
        Stage::Auth,
        started_at,
        deadline,
        &mut cancellation,
    )
    .await?
    {
        Output::Auth => {}
        _ => return Err(ClaudeError::Protocol),
    }
    let cooldown = ENDPOINT_COOLDOWN
        .lock()
        .expect("endpoint cooldown")
        .get(&executable)
        .copied()
        .filter(|until| *until > Instant::now());
    let until = match cooldown {
        Some(until) => until,
        None => match run(
            &executable,
            child_path,
            Stage::Quota,
            started_at,
            deadline,
            &mut cancellation,
        )
        .await
        {
            Ok(Output::Snapshot(snapshot)) => return Ok(snapshot),
            Ok(_) => return Err(ClaudeError::Protocol),
            Err(ClaudeError::RateLimited { retry_after_secs }) => {
                let now = Instant::now();
                let until = now + Duration::from_secs(u64::from(retry_after_secs));
                let mut cooldowns = ENDPOINT_COOLDOWN.lock().expect("endpoint cooldown");
                cooldowns.retain(|_, until| *until > now);
                cooldowns.insert(executable.clone(), until);
                until
            }
            Err(error) => return Err(error),
        },
    };
    // The usage endpoint is throttled. The same plan windows arrive as response
    // headers of a model request, which that limit does not cover.
    match run(
        &executable,
        child_path,
        Stage::Probe,
        now_ms(),
        Instant::now() + READ_TIMEOUT,
        &mut cancellation,
    )
    .await
    {
        Ok(Output::Snapshot(snapshot)) if !snapshot.windows.is_empty() => Ok(snapshot),
        Err(ClaudeError::Cancelled) => Err(ClaudeError::Cancelled),
        // Counted after the probe and rounded up, so the pause ends with the cooldown.
        _ => {
            let left = until.saturating_duration_since(Instant::now());
            let secs = left.as_secs() + u64::from(left.subsec_nanos() > 0);
            Err(ClaudeError::RateLimited {
                retry_after_secs: u32::try_from(secs).unwrap_or(u32::MAX).max(1),
            })
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Version,
    Auth,
    Login,
    Quota,
    Probe,
}
enum Output {
    Version(String),
    Auth,
    Login,
    Snapshot(ProviderSnapshot),
}
fn auth_status(bytes: &[u8]) -> Result<Output, ClaudeError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| ClaudeError::Protocol)?;
    match value["loggedIn"].as_bool() {
        Some(false) => Err(ClaudeError::NotAuthenticated),
        Some(true)
            if value["apiProvider"] == "firstParty"
                && matches!(
                    value["authMethod"].as_str(),
                    Some("claude.ai" | "oauth_token")
                ) =>
        {
            Ok(Output::Auth)
        }
        Some(true) => Err(ClaudeError::UnsupportedAccount),
        None => Err(ClaudeError::Protocol),
    }
}
async fn run(
    executable: &Path,
    child_path: &OsStr,
    stage: Stage,
    started_at: u64,
    deadline: Instant,
    cancellation: &mut Receiver<()>,
) -> Result<Output, ClaudeError> {
    if !matches!(
        cancellation.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ) {
        return Err(ClaudeError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(ClaudeError::Timeout);
    }
    let mut command = crate::process::command(executable);
    command.env("PATH", child_path);
    // Keep quota/feature-flag traffic available and disable optional traffic
    // individually. No global environment or user settings are modified.
    command.env_remove("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC");
    for (name, value) in [
        ("DISABLE_TELEMETRY", "1"),
        ("DISABLE_ERROR_REPORTING", "1"),
        ("DISABLE_AUTOUPDATER", "1"),
        ("CLAUDE_CODE_DISABLE_BACKGROUND_TASKS", "1"),
        ("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1"),
        ("CLAUDE_CODE_DISABLE_CLAUDE_MDS", "1"),
        ("ENABLE_CLAUDEAI_MCP_SERVERS", "false"),
    ] {
        command.env(name, value);
    }
    command
        .args(match stage {
            Stage::Version => &["--version"][..],
            Stage::Auth => &["auth", "status", "--json"][..],
            Stage::Login => &["auth", "login", "--claudeai"][..],
            Stage::Quota | Stage::Probe => ISOLATION_ARGS,
        })
        .args(match stage {
            Stage::Quota => QUOTA_ARGS,
            Stage::Probe => PROBE_ARGS,
            _ => &[][..],
        })
        .current_dir(std::env::temp_dir())
        .stdin(if stage == Stage::Quota {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    if stage == Stage::Quota {
        command.args(["--debug-file", "/dev/stderr"]);
    }
    #[cfg(windows)]
    if stage == Stage::Quota {
        command.arg("--debug");
    }
    crate::process::isolate(&mut command);
    let child = command.spawn().map_err(|_| ClaudeError::Io)?;
    #[cfg(windows)]
    let mut child = child;
    #[cfg(windows)]
    let job = match crate::process::ProcessJob::attach(&child) {
        Ok(job) => job,
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err(ClaudeError::Io);
        }
    };
    #[cfg(unix)]
    let pid = child.id().expect("new child has PID") as libc::pid_t;
    let mut owner = Owner {
        child: Some(child),
        #[cfg(unix)]
        pid: Some(pid),
        #[cfg(windows)]
        job,
        killed: false,
    };
    let result = {
        let child = owner.child.as_mut().expect("owned child");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let stdin = child.stdin.take();
        let exchange = timeout_at(deadline, async {
            if let Some(mut stdin) = stdin {
                // One control request, then EOF: no prompt, so no model turn.
                stdin
                    .write_all(&usage_request())
                    .await
                    .map_err(|_| ClaudeError::Io)?;
            }
            let output = read_stdout(stdout, stage, started_at);
            let diagnostics = read_stderr(stderr);
            tokio::pin!(output);
            tokio::pin!(diagnostics);
            let (result, details) = tokio::select! {
                biased;
                details=&mut diagnostics=>{let details=details?;(output.await,details)}
                result=&mut output=>{
                    // The CLI reports success even on quota HTTP errors. Give its
                    // asynchronous logger a short, bounded opportunity to flush
                    // before killing the still-owned (unreaped) process group.
                    let empty=matches!(&result,Ok(Output::Snapshot(p)) if p.windows.is_empty());
                    let drained=if stage==Stage::Quota&&empty {tokio::time::timeout(Duration::from_millis(250),&mut diagnostics).await.ok()} else {None};
                    owner.kill_group();
                    let details=match drained {Some(details)=>details?,None=>diagnostics.await?};
                    (result,details)
                }
            };
            let result = result?;
            if matches!(&result,Output::Snapshot(p) if p.windows.is_empty()) && details.rate_limited
            {
                return Err(ClaudeError::RateLimited {
                    retry_after_secs: details.retry_after_secs.unwrap_or(300),
                });
            }
            Ok(result)
        });
        tokio::pin!(exchange);
        tokio::select! {
            biased;
            _ = cancellation.recv() => Err(ClaudeError::Cancelled),
            result = &mut exchange => result.unwrap_or(Err(ClaudeError::Timeout)),
        }
    };
    // Never reap before group kill: the unreaped PID reserves the group identity.
    let status = owner.cleanup().await;
    #[cfg(windows)]
    let _ = status;
    #[cfg(unix)]
    if status.code().is_some() && !status.success() && result.is_ok() {
        return Err(ClaudeError::Command);
    }
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
                // SAFETY: our isolated, positive, unreaped child PID is reserved.
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
            }
            self.killed = true;
        }
    }
    async fn cleanup(&mut self) -> std::process::ExitStatus {
        self.kill_group();
        loop {
            if let Ok(status) = self.child.as_mut().expect("owned child").wait().await {
                #[cfg(unix)]
                {
                    self.pid = None;
                }
                self.child = None;
                return status;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
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
        }
        #[cfg(unix)]
        {
            self.pid = None;
        }
    }
}

#[derive(Default)]
struct Diagnostics {
    rate_limited: bool,
    retry_after_secs: Option<u32>,
    line: Vec<u8>,
    overflow: bool,
}
impl Diagnostics {
    fn feed(&mut self, bytes: &[u8]) {
        for byte in bytes {
            if *byte == b'\n' {
                self.finish_line();
            } else if !self.overflow {
                if self.line.len() < 8192 {
                    self.line.push(*byte);
                } else {
                    self.line.clear();
                    self.overflow = true;
                }
            }
        }
    }
    fn finish_line(&mut self) {
        if !self.overflow {
            let line = String::from_utf8_lossy(&self.line);
            // Exact pinned usage markers only. Never retain/forward raw lines,
            // URL, request body, account data, headers or authentication material.
            if let Some(offset) =
                line.find("fetchUtilization: 429 remembered for this bearer; not asking again for ")
            {
                self.rate_limited = true;
                let text = &line[offset
                    + "fetchUtilization: 429 remembered for this bearer; not asking again for "
                        .len()..];
                let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
                if text[digits.len()..].starts_with('s') {
                    self.retry_after_secs = digits
                        .parse::<u32>()
                        .ok()
                        .filter(|value| (1..=3600).contains(value));
                }
            }
            if line.contains("Failed to load usage data:") && line.contains("status code 429") {
                self.rate_limited = true;
            }
        }
        self.line.clear();
        self.overflow = false;
    }
}
async fn read_stderr(mut stderr: ChildStderr) -> Result<Diagnostics, ClaudeError> {
    let mut buffer = [0; 4096];
    let mut total = 0;
    let mut diagnostics = Diagnostics::default();
    loop {
        let capacity = buffer.len().min(MAX_STREAM - total);
        if capacity == 0 {
            return Err(ClaudeError::ResourceLimit);
        }
        let n = stderr
            .read(&mut buffer[..capacity])
            .await
            .map_err(|_| ClaudeError::Io)?;
        if n == 0 {
            diagnostics.finish_line();
            return Ok(diagnostics);
        }
        total += n;
        diagnostics.feed(&buffer[..n]);
    }
}

async fn read_stdout(
    mut stdout: ChildStdout,
    stage: Stage,
    started_at: u64,
) -> Result<Output, ClaudeError> {
    let mut buffer = [0; 4096];
    let mut frame = Vec::new();
    let mut bytes = 0;
    let mut count = 0;
    let json = matches!(stage, Stage::Quota | Stage::Probe);
    let mut windows = None;
    let mut record = |value: Value| match stage {
        Stage::Probe => accept_probe(&mut windows, value, started_at),
        _ => accept(value, started_at),
    };
    loop {
        let capacity = buffer.len().min(MAX_STREAM - bytes);
        if capacity == 0 {
            return Err(ClaudeError::ResourceLimit);
        }
        let n = stdout
            .read(&mut buffer[..capacity])
            .await
            .map_err(|_| ClaudeError::Io)?;
        if n == 0 {
            match stage {
                Stage::Version => {
                    return String::from_utf8(frame)
                        .map(Output::Version)
                        .map_err(|_| ClaudeError::Protocol)
                }
                Stage::Auth => return auth_status(&frame),
                Stage::Login => return Err(ClaudeError::Command),
                Stage::Quota | Stage::Probe => {}
            }
            // Accept a final complete JSON record without a trailing newline.
            if !frame.is_empty() {
                if count >= MAX_FRAMES {
                    return Err(ClaudeError::ResourceLimit);
                }
                if let Some(snapshot) = record(decode(&frame)?)? {
                    return Ok(Output::Snapshot(snapshot));
                }
            }
            return Err(ClaudeError::Protocol);
        }
        bytes += n;
        for &byte in &buffer[..n] {
            if byte == b'\n' && stage == Stage::Login {
                if frame.strip_suffix(b"\r").unwrap_or(&frame) == b"Login successful." {
                    return Ok(Output::Login);
                }
                frame.clear();
            } else if byte == b'\n' && json {
                count += 1;
                if count > MAX_FRAMES {
                    return Err(ClaudeError::ResourceLimit);
                }
                if let Some(snapshot) = record(decode(&frame)?)? {
                    return Ok(Output::Snapshot(snapshot));
                }
                frame.clear();
            } else {
                let max = if json { MAX_FRAME } else { 4096 };
                if frame.len() == max {
                    return Err(ClaudeError::ResourceLimit);
                }
                frame.push(byte);
            }
        }
    }
}
fn decode(frame: &[u8]) -> Result<Value, ClaudeError> {
    serde_json::from_slice(frame).map_err(|_| ClaudeError::Protocol)
}

/// Probe stream: keeps the last `unifiedWindows`; the model reply itself is ignored.
fn accept_probe(
    windows: &mut Option<Value>,
    value: Value,
    started_at: u64,
) -> Result<Option<ProviderSnapshot>, ClaudeError> {
    match value.get("type").and_then(Value::as_str) {
        Some("rate_limit_event") => {
            if let Some(found) = value["rate_limit_info"].get("unifiedWindows") {
                *windows = Some(found.clone());
            }
            Ok(None)
        }
        Some("result") => header_windows(windows.take(), started_at).map(Some),
        Some(_) => Ok(None),
        None => Err(ClaudeError::Protocol),
    }
}

/// Structured `/usage` data via the CLI control protocol (SDK `get_usage`).
fn usage_request() -> Vec<u8> {
    let mut line = serde_json::json!({
        "type": "control_request",
        "request_id": REQUEST_ID,
        "request": {"subtype": "get_usage", "skip_behaviors": true},
    })
    .to_string()
    .into_bytes();
    line.push(b'\n');
    line
}

fn accept(value: Value, started_at: u64) -> Result<Option<ProviderSnapshot>, ClaudeError> {
    let object = value.as_object().ok_or(ClaudeError::Protocol)?;
    match object.get("type").and_then(Value::as_str) {
        Some("system")
            if object.get("subtype").and_then(Value::as_str) == Some("ui_invalidate") =>
        {
            // Pinned CLI may announce cache/policy changes before answering.
            // It is not quota and is bounded by the frame/stream budgets.
            Ok(None)
        }
        Some("control_response") if value["response"]["request_id"] == REQUEST_ID => {
            match value["response"]["subtype"].as_str() {
                Some("error") => Err(ClaudeError::Command),
                Some("success") => {
                    let usage = &value["response"]["response"];
                    // Zero session totals confirm that no model request ran.
                    if usage["session"]["total_cost_usd"].as_f64() != Some(0.0)
                        || !usage["session"]["model_usage"]
                            .as_object()
                            .is_some_and(|models| models.is_empty())
                    {
                        return Err(ClaudeError::Protocol);
                    }
                    // Seeded (stale) fallbacks lack generic `limits`; normalize ignores
                    // legacy windows, so such answers become unavailable.
                    normalize(Some(usage.clone()), started_at).map(Some)
                }
                _ => Err(ClaudeError::Protocol),
            }
        }
        _ => Err(ClaudeError::Protocol),
    }
}

#[cfg(all(test, unix))]
pub(crate) mod tests;
