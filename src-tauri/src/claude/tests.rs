use super::*;
use crate::model::ProviderId;
use serde_json::json;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

const NOW: u64 = 1_791_656_400_000;
const SECRET: &str = "SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN";
fn report() -> Value {
    json!({
        "account":SECRET,"behaviors":SECRET,"subscription_type":SECRET,
        "session":{"total_cost_usd":0,"model_usage":{},"total_duration_ms":SECRET},
        "rate_limits":{"extra_usage":{"utilization":99},"limits":[
            {"kind":"session","group":"session","percent":13.75,"resets_at":"2026-10-10T22:20:00.392663+03:00","is_active":false},
            {"kind":"weekly_all","group":"weekly","percent":28,"resets_at":"2026-10-15T00:00:00Z","is_active":true},
            {"kind":"weekly_scoped","group":"weekly","percent":98,"resets_at":"2026-10-15T00:00:00Z","scope":{"model":{"display_name":SECRET}}}
        ]}
    })
}

#[test]
fn normalizes_only_global_windows_without_identity_or_costs() {
    let snapshot = normalize(Some(report()), NOW).unwrap();
    assert_eq!(snapshot.id, ProviderId::Claude);
    assert_eq!(snapshot.status, ProviderStatus::Current);
    assert_eq!(snapshot.windows.len(), 2);
    assert_eq!(snapshot.windows[0].used_percent, 13.75);
    assert_eq!(snapshot.windows[0].resets_at, Some(1_791_660_000_392));
    assert_eq!(snapshot.remaining(NOW), Some(72.0));
    assert!(!serde_json::to_string(&snapshot).unwrap().contains(SECRET));
}

#[test]
fn unavailable_is_empty_and_has_no_observation_timestamp() {
    for value in [
        None,
        Some(Value::Null),
        Some(json!({"rate_limits":null})),
        Some(json!({"rate_limits":{"limits":null}})),
        Some(json!({"rate_limits":{"limits":[]}})),
        Some(json!({"rate_limits":{"five_hour":{"utilization":99}}})),
    ] {
        let snapshot = normalize(value, NOW).unwrap();
        assert_eq!(snapshot.status, ProviderStatus::Unavailable);
        assert_eq!(snapshot.fetched_at, None);
        assert!(snapshot.windows.is_empty());
        assert_eq!(snapshot.remaining(NOW), None);
    }
}

#[test]
fn freshness_subtracts_cli_cache_allowance_and_never_renews_expired_resets() {
    let mut value = report();
    value["rate_limits"]["limits"][0]["resets_at"] = json!("1970-01-01T00:00:01Z");
    value["rate_limits"]["limits"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    let snapshot = normalize(Some(value), NOW).unwrap();
    assert_eq!(snapshot.fetched_at, Some(NOW - 60_000));
    assert_eq!(snapshot.remaining(NOW), None);
    let snapshot = normalize(Some(report()), NOW).unwrap();
    assert_eq!(snapshot.remaining(NOW + 240_001), None);
}

#[test]
fn rejects_bad_percent_reset_group_scope_and_duplicate_windows() {
    for (field, bad) in [
        ("percent", json!(-1)),
        ("percent", json!(100.01)),
        ("percent", json!("13")),
        ("percent", Value::Null),
        ("resets_at", json!(123)),
        ("resets_at", json!("2026-02-30T00:00:00Z")),
        ("resets_at", json!("2026-10-10 at 10pm")),
        ("resets_at", json!("1969-12-31T23:59:59Z")),
        ("group", json!("weekly")),
        ("scope", json!({"model":{"display_name":SECRET}})),
    ] {
        let mut value = report();
        value["rate_limits"]["limits"][0][field] = bad;
        assert_eq!(
            normalize(Some(value), NOW).unwrap_err(),
            ClaudeError::InvalidResponse
        );
    }
    let mut value = report();
    let duplicate = value["rate_limits"]["limits"][0].clone();
    value["rate_limits"]["limits"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert_eq!(
        normalize(Some(value), NOW).unwrap_err(),
        ClaudeError::InvalidResponse
    );
}

#[test]
fn null_reset_and_weekly_only_do_not_invent_a_short_window() {
    let snapshot = normalize(
        Some(json!({"rate_limits":{"limits":[
            {"kind":"weekly_all","group":"weekly","percent":100,"resets_at":null}
        ]}})),
        NOW,
    )
    .unwrap();
    assert_eq!(snapshot.windows.len(), 1);
    assert_eq!(snapshot.windows[0].duration_mins, 10080);
    assert_eq!(snapshot.windows[0].resets_at, None);
    assert_eq!(snapshot.remaining(NOW), Some(0.0));
}

fn response() -> Value {
    json!({"type":"control_response","response":{"subtype":"success",
        "request_id":"usage","response":report()}})
}
#[test]
fn usage_control_response_is_the_only_accepted_record() {
    let invalidate = json!({"type":"system","subtype":"ui_invalidate","event":SECRET});
    assert!(accept(invalidate, NOW).unwrap().is_none());
    let snapshot = accept(response(), NOW).unwrap().unwrap();
    assert_eq!(snapshot.remaining(NOW), Some(72.0));
    assert!(!serde_json::to_string(&snapshot).unwrap().contains(SECRET));
    for value in [
        json!({"type":"system","subtype":"init","tools":[],"session_id":SECRET}),
        json!({"type":"assistant","message":{"content":SECRET}}),
        json!({"type":"result","subtype":"success","result":SECRET}),
        json!({"type":"control_request","request_id":SECRET,"request":{"subtype":"can_use_tool"}}),
        json!({"type":"control_response","response":{"subtype":"other","request_id":"usage"}}),
        json!([]),
    ] {
        assert_eq!(accept(value, NOW).unwrap_err(), ClaudeError::Protocol);
    }
}
#[test]
fn rejects_foreign_request_id_and_any_session_inference() {
    let mut value = response();
    value["response"]["request_id"] = json!("other");
    assert_eq!(accept(value, NOW).unwrap_err(), ClaudeError::Protocol);
    for (key, bad) in [
        ("total_cost_usd", json!(0.001)),
        ("total_cost_usd", Value::Null),
        ("model_usage", json!({"claude-fable":{}})),
        ("model_usage", Value::Null),
    ] {
        let mut value = response();
        value["response"]["response"]["session"][key] = bad;
        assert_eq!(accept(value, NOW).unwrap_err(), ClaudeError::Protocol);
    }
    let mut value = response();
    value["response"]["response"]
        .as_object_mut()
        .unwrap()
        .remove("session");
    assert_eq!(accept(value, NOW).unwrap_err(), ClaudeError::Protocol);
}
#[test]
fn seeded_fallback_and_missing_quota_are_unavailable_and_errors_are_static() {
    // The pinned CLI strips generic `limits` from fallbacks seeded by headers or
    // a persisted snapshot; legacy windows alone do not prove a fresh read.
    for rate_limits in [
        json!({"five_hour":{"utilization":99,"resets_at":"2026-10-10T19:20:00+00:00"},
            "model_scoped":[{"display_name":SECRET,"utilization":2,"resets_at":null}]}),
        Value::Null,
    ] {
        let mut value = response();
        value["response"]["response"]["rate_limits"] = rate_limits;
        let snapshot = accept(value, NOW).unwrap().unwrap();
        assert_eq!(snapshot.status, ProviderStatus::Unavailable);
        assert!(snapshot.windows.is_empty());
    }
    let error = accept(
        json!({"type":"control_response","response":{"subtype":"error",
            "request_id":"usage","error":SECRET}}),
        NOW,
    )
    .unwrap_err();
    assert_eq!(error, ClaudeError::Command);
    assert!(!format!("{error} {error:?}").contains(SECRET));
}
fn headers_event() -> Value {
    json!({"type":"rate_limit_event","session_id":SECRET,"rate_limit_info":{
    "status":"allowed","rateLimitType":"five_hour","overageDisabledReason":SECRET,
    "unifiedWindows":{
        "five_hour":{"utilization":0.07,"resetsAt":1_791_678_000_u64},
        "seven_day":{"utilization":0.3,"resetsAt":1_792_022_400_u64},
        "seven_day_fable":{"utilization":SECRET}
    }}})
}
fn probe_result() -> Value {
    json!({"type":"result","subtype":"success","is_error":false,"result":SECRET})
}
fn probe(events: Vec<Value>) -> Result<ProviderSnapshot, ClaudeError> {
    let mut windows = None;
    for value in events {
        assert!(accept_probe(&mut windows, value, NOW)?.is_none());
    }
    Ok(accept_probe(&mut windows, probe_result(), NOW)?.unwrap())
}
#[test]
fn probe_headers_become_fresh_session_and_weekly_windows() {
    let snapshot = probe(vec![
        json!({"type":"system","subtype":"init","session_id":SECRET}),
        json!({"type":"assistant","message":{"content":SECRET}}),
        headers_event(),
    ])
    .unwrap();
    assert_eq!(snapshot.status, ProviderStatus::Current);
    assert_eq!(snapshot.fetched_at, Some(NOW));
    let windows: Vec<_> = snapshot
        .windows
        .iter()
        .map(|w| (w.id.as_str(), w.used_percent, w.resets_at, w.duration_mins))
        .collect();
    assert_eq!(
        windows,
        [
            ("claude:session", 7.0, Some(1_791_678_000_000), 300),
            ("claude:weekly_all", 30.0, Some(1_792_022_400_000), 10080),
        ]
    );
    assert!(!serde_json::to_string(&snapshot).unwrap().contains(SECRET));
}
#[test]
fn probe_without_valid_headers_never_invents_quota() {
    for events in [
        vec![],
        vec![json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}})],
    ] {
        let snapshot = probe(events).unwrap();
        assert_eq!(snapshot.status, ProviderStatus::Unavailable);
        assert_eq!(snapshot.fetched_at, None);
    }
    for (field, bad) in [
        ("utilization", json!(-0.01)),
        ("utilization", json!("0.5")),
        ("utilization", Value::Null),
        ("resetsAt", json!(-1)),
        ("resetsAt", json!("soon")),
    ] {
        let mut event = headers_event();
        event["rate_limit_info"]["unifiedWindows"]["five_hour"][field] = bad;
        assert_eq!(
            probe(vec![event]).unwrap_err(),
            ClaudeError::InvalidResponse
        );
    }
    let mut event = headers_event();
    event["rate_limit_info"]["unifiedWindows"]["five_hour"]["utilization"] = json!(1.2);
    assert_eq!(probe(vec![event]).unwrap().windows[0].used_percent, 100.0);
    assert_eq!(
        accept_probe(&mut None, json!([]), NOW).unwrap_err(),
        ClaudeError::Protocol
    );
}
#[test]
fn failed_probe_turn_still_reports_headers_observed_by_its_own_request() {
    // A fresh --no-session-persistence process only learns windows from its own
    // responses, so a failed model turn (e.g. exhausted SDK credit) is not stale.
    let failed = json!({"type":"result","subtype":"error_during_execution","is_error":true});
    let mut windows = None;
    accept_probe(&mut windows, headers_event(), NOW).unwrap();
    let snapshot = accept_probe(&mut windows, failed.clone(), NOW)
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.status, ProviderStatus::Current);
    let snapshot = accept_probe(&mut None, failed, NOW).unwrap().unwrap();
    assert_eq!(snapshot.status, ProviderStatus::Unavailable);
}
#[test]
fn malformed_nested_schema_and_missing_reset_never_become_zero() {
    for value in [
        json!([]),
        json!({"rate_limits":[]}),
        json!({"rate_limits":{"limits":{}}}),
        json!({"rate_limits":{"limits":[null]}}),
        json!({"rate_limits":{"limits":[{"kind":"session","group":"session","percent":0}]}}),
    ] {
        assert_eq!(
            normalize(Some(value), NOW).unwrap_err(),
            ClaudeError::InvalidResponse
        );
    }
    assert_eq!(
        normalize(Some(report()), 30_000).unwrap().fetched_at,
        Some(0)
    );
}

pub(crate) struct Fake {
    pub(crate) dir: PathBuf,
    pub(crate) executable: PathBuf,
}
impl Fake {
    pub(crate) fn new(mode: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "kyl-claude-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let executable = dir.join("synthetic-cli");
        // End-to-end callers judge freshness by the wall clock, not `NOW`.
        let mut response = response();
        for (row, hours) in response["response"]["response"]["rate_limits"]["limits"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip([1, 24, 24])
        {
            let at = chrono::DateTime::from_timestamp_millis((now_ms() + hours * 3_600_000) as i64);
            row["resets_at"] = json!(at.unwrap().to_rfc3339());
        }
        let header = format!(
            "#!/usr/bin/python3\nMODE={mode:?}\nDIRECTORY={:?}\nRESPONSE={:?}\n",
            dir.to_str().unwrap(),
            response.to_string()
        );
        fs::write(&executable, header + SCRIPT).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        Self { dir, executable }
    }
    async fn pids(&self, phase: &str) -> Vec<i32> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(text) = fs::read_to_string(self.dir.join(format!("{phase}-ready"))) {
                return text
                    .split_whitespace()
                    .map(|value| value.parse().unwrap())
                    .collect();
            }
            assert!(Instant::now() < deadline, "synthetic CLI did not start");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    pub(crate) async fn cleaned(&self, phase: &str) {
        let pids = self.pids(phase).await;
        let deadline = Instant::now() + Duration::from_secs(10);
        while !pids.iter().all(|pid| unsafe { libc::kill(*pid, 0) } == -1) {
            assert!(
                Instant::now() < deadline,
                "owned synthetic child still running"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    fn launches(&self) -> Vec<String> {
        fs::read_to_string(self.dir.join("launches"))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
const SCRIPT: &str = r#"
import json, os, sys, time, signal, tempfile
phase = 'version' if sys.argv[1:] == ['--version'] else 'auth' if sys.argv[1:] == ['auth','status','--json'] else 'login' if sys.argv[1:] == ['auth','login','--claudeai'] else 'probe' if '--model' in sys.argv else 'quota'
LIMITED = ['rate_limited','rate_limited_headers','probe_cancel','slow_probe']
ISOLATION = ['--safe-mode','--print','--output-format','stream-json','--verbose','--no-session-persistence','--tools','','--strict-mcp-config','--mcp-config','{"mcpServers":{}}','--no-chrome','--settings','{"disableAllHooks":true,"disableClaudeAiConnectors":true,"autoContinueAtUsageLimit":false}']
def send(value): print(json.dumps(value),flush=True)
with open(os.path.join(DIRECTORY,'launches'),'a') as f: f.write(phase+'\n')
assert os.getpgrp() == os.getpid()
assert os.path.realpath(os.getcwd()) == os.path.realpath(tempfile.gettempdir())
request = sys.stdin.read()
if phase == 'quota':
    assert request.endswith('\n') and request.count('\n') == 1
    assert json.loads(request) == {'type':'control_request','request_id':'usage','request':{'subtype':'get_usage','skip_behaviors':True}}
else:
    assert request == ''
assert 'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC' not in os.environ
for name in ['DISABLE_TELEMETRY','DISABLE_ERROR_REPORTING','DISABLE_AUTOUPDATER','CLAUDE_CODE_DISABLE_BACKGROUND_TASKS','CLAUDE_CODE_DISABLE_AUTO_MEMORY','CLAUDE_CODE_DISABLE_CLAUDE_MDS']:
    assert os.environ[name] == '1'
assert os.environ['ENABLE_CLAUDEAI_MCP_SERVERS'] == 'false'
pids=[os.getpid()]
if phase == 'quota' or MODE in ['version_cancel','auth_cancel','login_cancel']:
    signal.signal(signal.SIGCHLD,signal.SIG_IGN)
    helper=os.fork()
    if helper == 0:
        os.close(1)
        os.close(2)
        while True: time.sleep(1)
    pids.append(helper)
with open(os.path.join(DIRECTORY,phase+'-ready.tmp'),'w') as f: f.write(' '.join(map(str,pids)))
os.rename(os.path.join(DIRECTORY,phase+'-ready.tmp'),os.path.join(DIRECTORY,phase+'-ready'))
if phase == 'version':
    if MODE == 'retarget':
        os.unlink(os.path.join(DIRECTORY,'link'))
        os.symlink(os.path.join(DIRECTORY,'not-this-cli'),os.path.join(DIRECTORY,'link'))
    if MODE == 'version_cancel':
        while True: time.sleep(1)
    if MODE == 'version_flood':
        print('x'*5000,flush=True)
        while True: time.sleep(1)
    print({'wrong_version':'2.1.295','newer_version':'2.1.304'}.get(MODE,'2.1.296')+' (Claude Code)',flush=True)
    sys.exit(0)
if phase == 'auth':
    if MODE == 'auth_cancel':
        while True: time.sleep(1)
    if MODE == 'auth_invalid': print('SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN',flush=True)
    else: print(json.dumps({'loggedIn':MODE != 'signed_out','authMethod':'api_key' if MODE == 'api_account' else 'claude.ai','apiProvider':'firstParty','email':'SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN'}),flush=True)
    sys.exit(1 if MODE == 'signed_out' else 0)
if phase == 'login':
    if MODE == 'login_cancel':
        while True: time.sleep(1)
    if MODE == 'login_error':
        print('SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN',flush=True)
        sys.exit(1)
    print('Opening browser to sign in…',flush=True)
    print('Login successful.',flush=True)
    sys.exit(0)
if phase == 'probe':
    assert sys.argv[1:] == ISOLATION+['--max-turns','1','--model','haiku','--system-prompt','Reply with a single dot.','.']
    if MODE == 'probe_cancel':
        while True: time.sleep(1)
    if MODE == 'slow_probe': time.sleep(1.5)
    send({'type':'system','subtype':'init','session_id':'SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN'})
    send({'type':'assistant','message':{'content':'SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN'}})
    if MODE == 'rate_limited_headers':
        send({'type':'rate_limit_event','rate_limit_info':{'status':'allowed','unifiedWindows':{'five_hour':{'utilization':0.07,'resetsAt':1791678000},'seven_day':{'utilization':0.3,'resetsAt':1792022400}}}})
    send({'type':'result','subtype':'success','is_error':False,'result':'SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN'})
    while True: time.sleep(1)
assert sys.argv[1:] == ISOLATION+['--input-format','stream-json','--max-turns','0','--debug-file','/dev/stderr']
with open(os.path.join(DIRECTORY,'child-path'),'w') as f: f.write(os.environ['PATH'])
if MODE == 'cancel' or MODE == 'timeout':
    while True: time.sleep(1)
if MODE == 'eof': sys.exit(0)
if MODE == 'invalid': print('SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN',flush=True)
elif MODE == 'frame_flood': print('x'*65537,flush=True)
elif MODE == 'stderr_flood': sys.stderr.write('x'*262145); sys.stderr.flush()
else:
    for _ in range(65 if MODE == 'many_frames' else 6 if MODE == 'total_flood' else 1):
        send({'type':'system','subtype':'ui_invalidate','event':'x'*(50000 if MODE == 'total_flood' else 1)})
    if MODE == 'unexpected': send({'type':'system','subtype':'init','tools':[]})
    if MODE == 'stderr_ok': sys.stderr.write('x'*250000); sys.stderr.flush()
    response=json.loads(RESPONSE)
    usage=response['response']['response']
    if MODE in LIMITED+['late_rate_limited']:
        if MODE in LIMITED: sys.stderr.write('[DEBUG] fetchUtilization: 429 remembered for this bearer; not asking again for 300s\n[DEBUG] Failed to load usage data: Request failed with status code 429\n');sys.stderr.flush()
        usage['rate_limits']={'five_hour':{'utilization':17,'resets_at':'2026-10-10T19:20:00+00:00'},'extra_usage':None}
    if MODE == 'no_quota':
        usage['rate_limits_available']=False
        usage['rate_limits']=None
    if MODE == 'inference': usage['session']['total_cost_usd']=0.01
    if MODE == 'command_error': response['response']={'subtype':'error','request_id':'usage','error':'SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN'}
    if MODE == 'partial':
        sys.stdout.write('{'); sys.stdout.flush()
    elif MODE == 'no_newline': sys.stdout.write(json.dumps(response)); sys.stdout.flush(); sys.exit(0)
    else: send(response)
    if MODE == 'late_rate_limited':
        time.sleep(0.05)
        sys.stderr.write('[DEBUG] fetchUtilization: 429 remembered for this bearer; not asking again for 900s\n');sys.stderr.flush()
while True: time.sleep(1)
"#;
async fn fake_read(fake: &Fake) -> Result<ProviderSnapshot, ClaudeError> {
    let (_cancel, receiver) = tauri::async_runtime::channel(1);
    read(&fake.executable, OsStr::new("/usr/bin:/bin"), receiver).await
}
#[tokio::test(flavor = "current_thread")]
async fn exact_local_read_cleans_helper_and_preserves_global_environment() {
    let path = std::env::var_os("PATH");
    let home = std::env::var_os("HOME");
    let fake = Fake::new("normal");
    let snapshot = fake_read(&fake).await.unwrap();
    assert_eq!(snapshot.windows.len(), 2);
    assert!(!serde_json::to_string(&snapshot).unwrap().contains(SECRET));
    fake.cleaned("version").await;
    fake.cleaned("quota").await;
    assert_eq!(fake.launches(), ["version", "auth", "quota"]);
    assert_eq!(
        fs::read_to_string(fake.dir.join("child-path")).unwrap(),
        "/usr/bin:/bin"
    );
    assert_eq!(std::env::var_os("PATH"), path);
    assert_eq!(std::env::var_os("HOME"), home);
}
#[tokio::test(flavor = "current_thread")]
async fn unsupported_version_never_runs_quota_command() {
    let fake = Fake::new("wrong_version");
    assert_eq!(
        fake_read(&fake).await.unwrap_err(),
        ClaudeError::UnsupportedVersion
    );
    assert_eq!(fake.launches(), ["version"]);
    fake.cleaned("version").await;
}
#[tokio::test(flavor = "current_thread")]
async fn newer_version_reads_quota() {
    let fake = Fake::new("newer_version");
    assert_eq!(fake_read(&fake).await.unwrap().windows.len(), 2);
    assert_eq!(fake.launches(), ["version", "auth", "quota"]);
    fake.cleaned("quota").await;
}
#[test]
fn accepts_only_minimum_or_newer_claude_versions() {
    for text in [
        "2.1.296 (Claude Code)",
        "2.1.304 (Claude Code)\n",
        "2.2.0 (Claude Code)",
        "3.0.0 (Claude Code)",
    ] {
        assert!(supported_version(text), "{text:?}");
    }
    for text in [
        "",
        "2.1.295 (Claude Code)",
        "1.9.999 (Claude Code)",
        "2.1.296",
        "2.1.296 (Other)",
        "2.1 (Claude Code)",
        "2.1.296.1 (Claude Code)",
        "v2.1.296 (Claude Code)",
        "2.1.296-beta (Claude Code)",
    ] {
        assert!(!supported_version(text), "{text:?}");
    }
}
#[tokio::test(flavor = "current_thread")]
async fn canonical_version_target_survives_a_retargeted_install_symlink() {
    let fake = Fake::new("retarget");
    let link = fake.dir.join("link");
    std::os::unix::fs::symlink(&fake.executable, &link).unwrap();
    let (_cancel, receiver) = tauri::async_runtime::channel(1);
    assert!(read(&link, OsStr::new("/usr/bin"), receiver).await.is_ok());
    assert_eq!(fake.launches(), ["version", "auth", "quota"]);
    fake.cleaned("quota").await;
}
#[tokio::test(flavor = "current_thread")]
async fn relative_or_missing_executables_return_only_static_io_error() {
    for path in [
        Path::new("relative-cli"),
        Path::new("/does-not-exist/SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN"),
    ] {
        let (_cancel, receiver) = tauri::async_runtime::channel(1);
        let error = read(path, OsStr::new("/usr/bin"), receiver)
            .await
            .unwrap_err();
        assert_eq!(error, ClaudeError::Io);
        assert!(!format!("{error:?} {error}").contains(SECRET));
    }
}
#[tokio::test(flavor = "current_thread")]
async fn cancellation_before_launch_creates_no_process() {
    let fake = Fake::new("normal");
    let (cancel, receiver) = tauri::async_runtime::channel(1);
    cancel.try_send(()).unwrap();
    assert_eq!(
        read(&fake.executable, OsStr::new("/usr/bin"), receiver)
            .await
            .unwrap_err(),
        ClaudeError::Cancelled
    );
    assert!(!fake.dir.join("launches").exists());
}
#[tokio::test(flavor = "current_thread")]
async fn cancellation_awaits_cleanup_during_all_subprocess_stages() {
    for (mode, phase) in [
        ("version_cancel", "version"),
        ("auth_cancel", "auth"),
        ("cancel", "quota"),
        ("probe_cancel", "probe"),
    ] {
        let fake = Fake::new(mode);
        let executable = fake.executable.clone();
        let (cancel, receiver) = tauri::async_runtime::channel(1);
        let task =
            tokio::spawn(async move { read(&executable, OsStr::new("/usr/bin"), receiver).await });
        fake.pids(phase).await;
        cancel.try_send(()).unwrap();
        assert_eq!(task.await.unwrap().unwrap_err(), ClaudeError::Cancelled);
        fake.cleaned(phase).await;
    }
}
#[tokio::test(flavor = "current_thread")]
async fn protocol_errors_are_static_and_reap_child_helpers() {
    for mode in ["invalid", "eof", "unexpected", "inference"] {
        let fake = Fake::new(mode);
        let error = fake_read(&fake).await.unwrap_err();
        assert_eq!(error, ClaudeError::Protocol, "{mode}");
        assert!(!format!("{error} {error:?}").contains(SECRET));
        fake.cleaned("quota").await;
    }
    let fake = Fake::new("command_error");
    assert_eq!(fake_read(&fake).await.unwrap_err(), ClaudeError::Command);
    fake.cleaned("quota").await;
}
#[tokio::test(flavor = "current_thread")]
async fn stdout_and_stderr_budgets_do_not_deadlock_or_leak() {
    for (mode, phase) in [
        ("version_flood", "version"),
        ("frame_flood", "quota"),
        ("stderr_flood", "quota"),
        ("many_frames", "quota"),
        ("total_flood", "quota"),
    ] {
        let fake = Fake::new(mode);
        assert_eq!(
            fake_read(&fake).await.unwrap_err(),
            ClaudeError::ResourceLimit,
            "{mode}"
        );
        fake.cleaned(phase).await;
    }
    let fake = Fake::new("stderr_ok");
    assert!(fake_read(&fake).await.is_ok());
    fake.cleaned("quota").await;
}
#[tokio::test(flavor = "current_thread")]
async fn terminal_record_without_newline_and_no_quota_are_supported() {
    for mode in ["no_newline", "no_quota"] {
        let fake = Fake::new(mode);
        let snapshot = fake_read(&fake).await.unwrap();
        assert_eq!(
            snapshot.status,
            if mode == "no_quota" {
                ProviderStatus::Unavailable
            } else {
                ProviderStatus::Current
            }
        );
        fake.cleaned("quota").await;
    }
}
#[tokio::test(flavor = "current_thread")]
async fn overall_deadline_and_partial_output_reap_the_owned_group() {
    let fake = Fake::new("partial");
    let started = Instant::now();
    assert_eq!(fake_read(&fake).await.unwrap_err(), ClaudeError::Timeout);
    assert!(started.elapsed() >= READ_TIMEOUT);
    assert!(started.elapsed() < READ_TIMEOUT + Duration::from_secs(2));
    fake.cleaned("quota").await;
}

#[test]
fn auth_status_rejects_signed_out_non_subscription_and_malformed_without_identity() {
    for (input, expected) in [
        (
            json!({"loggedIn":false,"email":SECRET}),
            ClaudeError::NotAuthenticated,
        ),
        (
            json!({"loggedIn":true,"authMethod":"api_key","apiProvider":"firstParty","email":SECRET}),
            ClaudeError::UnsupportedAccount,
        ),
        (
            json!({"loggedIn":"true","email":SECRET}),
            ClaudeError::Protocol,
        ),
    ] {
        let error = auth_status(input.to_string().as_bytes()).err().unwrap();
        assert_eq!(error, expected);
        assert!(!error.to_string().contains(SECRET));
    }
}
#[tokio::test(flavor = "current_thread")]
async fn no_login_runs_no_quota_and_returns_actionable_error() {
    for (mode, expected) in [
        ("signed_out", ClaudeError::NotAuthenticated),
        ("api_account", ClaudeError::UnsupportedAccount),
        ("auth_invalid", ClaudeError::Protocol),
    ] {
        let fake = Fake::new(mode);
        assert_eq!(fake_read(&fake).await.unwrap_err(), expected);
        assert_eq!(fake.launches(), ["version", "auth"]);
        fake.cleaned("auth").await;
    }
}
#[tokio::test(flavor = "current_thread")]
async fn explicit_sign_in_uses_official_subscription_login_then_reads_quota() {
    let fake = Fake::new("normal");
    let (_cancel, rx) = tauri::async_runtime::channel(1);
    let result = read_or_sign_in(&fake.executable, OsStr::new("/usr/bin:/bin"), rx, true)
        .await
        .unwrap();
    assert_eq!(result.windows.len(), 2);
    assert_eq!(fake.launches(), ["version", "login", "auth", "quota"]);
    fake.cleaned("quota").await;
}
#[tokio::test(flavor = "current_thread")]
async fn login_cancellation_and_failure_are_static_and_await_cleanup() {
    let fake = Fake::new("login_cancel");
    let executable = fake.executable.clone();
    let (cancel, rx) = tauri::async_runtime::channel(1);
    let task = tokio::spawn(async move {
        read_or_sign_in(&executable, OsStr::new("/usr/bin:/bin"), rx, true).await
    });
    fake.pids("login").await;
    cancel.try_send(()).unwrap();
    assert_eq!(task.await.unwrap().unwrap_err(), ClaudeError::Cancelled);
    fake.cleaned("login").await;
    let fake = Fake::new("login_error");
    let (_cancel, rx) = tauri::async_runtime::channel(1);
    assert_eq!(
        read_or_sign_in(&fake.executable, OsStr::new("/usr/bin:/bin"), rx, true)
            .await
            .unwrap_err(),
        ClaudeError::Command
    );
    fake.cleaned("login").await;
}

#[tokio::test(flavor = "current_thread")]
async fn http_429_seeded_fallback_is_an_error_not_a_quota() {
    let fake = Fake::new("rate_limited");
    let error = fake_read(&fake).await.unwrap_err();
    assert!(
        matches!(error, ClaudeError::RateLimited { retry_after_secs } if (299..=300).contains(&retry_after_secs)),
        "HTTP 429 without probe headers must stay a cooldown, not a quota: {error:?}"
    );
    assert_eq!(fake.launches(), ["version", "auth", "quota", "probe"]);
    fake.cleaned("quota").await;
    fake.cleaned("probe").await;
}

#[test]
fn probe_arguments_stay_byte_identical_for_prompt_cache_reuse() {
    // The CLI caches this prefix for 1 h: a cached probe costs ~16x less.
    // Any change here makes the next probe pay the full cache write again.
    let args: Vec<_> = ISOLATION_ARGS.iter().chain(PROBE_ARGS).copied().collect();
    assert_eq!(
        args,
        [
            "--safe-mode", "--print", "--output-format", "stream-json", "--verbose",
            "--no-session-persistence", "--tools", "", "--strict-mcp-config",
            "--mcp-config", "{\"mcpServers\":{}}", "--no-chrome", "--settings",
            "{\"disableAllHooks\":true,\"disableClaudeAiConnectors\":true,\"autoContinueAtUsageLimit\":false}",
            "--max-turns", "1", "--model", "haiku", "--system-prompt",
            "Reply with a single dot.", ".",
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn endpoint_cooldown_skips_the_usage_read_and_goes_straight_to_the_probe() {
    let fake = Fake::new("rate_limited_headers");
    fake_read(&fake).await.unwrap();
    fake_read(&fake).await.unwrap();
    assert_eq!(
        fake.launches(),
        ["version", "auth", "quota", "probe", "version", "auth", "probe"]
    );
    fake.cleaned("probe").await;
    let fake = Fake::new("rate_limited");
    fake_read(&fake).await.unwrap_err();
    let error = fake_read(&fake).await.unwrap_err();
    assert!(
        matches!(error, ClaudeError::RateLimited { retry_after_secs } if (1..=300).contains(&retry_after_secs)),
        "{error:?}"
    );
    assert_eq!(
        fake.launches(),
        ["version", "auth", "quota", "probe", "version", "auth", "probe"]
    );
    fake.cleaned("probe").await;
}

#[tokio::test(flavor = "current_thread")]
async fn returned_cooldown_keeps_counting_down_during_the_probe() {
    // 300 s were announced before a 1.5 s probe; the pause must still end on time.
    let fake = Fake::new("slow_probe");
    let error = fake_read(&fake).await.unwrap_err();
    assert!(
        matches!(error, ClaudeError::RateLimited { retry_after_secs } if (290..300).contains(&retry_after_secs)),
        "{error:?}"
    );
    fake.cleaned("probe").await;
}

#[tokio::test(flavor = "current_thread")]
async fn http_429_falls_back_to_fresh_headers_of_one_probe_request() {
    let fake = Fake::new("rate_limited_headers");
    let started = now_ms();
    let snapshot = fake_read(&fake).await.unwrap();
    assert_eq!(snapshot.status, ProviderStatus::Current);
    assert!(snapshot.fetched_at.unwrap() >= started);
    let used: Vec<_> = snapshot.windows.iter().map(|w| w.used_percent).collect();
    assert_eq!(used, [7.0, 30.0]);
    assert!(!serde_json::to_string(&snapshot).unwrap().contains(SECRET));
    assert_eq!(fake.launches(), ["version", "auth", "quota", "probe"]);
    fake.cleaned("quota").await;
    fake.cleaned("probe").await;
}

#[test]
fn diagnostic_parser_is_bounded_and_scoped_to_usage_without_retaining_raw_data() {
    let mut d = Diagnostics::default();
    d.feed(b"[DEBUG] unrelated network status code 429 SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN\n");
    assert!(!d.rate_limited);
    d.feed(b"[DEBUG] fetchUtilization: 429 remembered for this bearer; not asking again for ");
    d.feed(b"900s\n");
    assert!(d.rate_limited);
    assert_eq!(d.retry_after_secs, Some(900));
    assert!(d.line.is_empty());
    let mut d = Diagnostics::default();
    d.feed(b"[DEBUG] Failed to load usage data: Request failed with status code 429\n");
    assert!(d.rate_limited);
    assert_eq!(d.retry_after_secs, None);
    let mut d = Diagnostics::default();
    d.feed(&vec![b'x'; 250000]);
    d.finish_line();
    assert!(!d.rate_limited);
    assert!(d.line.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn late_quota_diagnostic_is_drained_before_cleanup_and_respects_retry_after() {
    let fake = Fake::new("late_rate_limited");
    let error = fake_read(&fake).await.unwrap_err();
    assert!(
        matches!(error, ClaudeError::RateLimited { retry_after_secs } if (899..=900).contains(&retry_after_secs)),
        "{error:?}"
    );
    fake.cleaned("quota").await;
    assert!(!error.to_string().contains(SECRET));
}
