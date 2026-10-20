use super::*;
use crate::model::{ProviderId, ProviderStatus, STALE_AFTER_MS};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const NOW: u64 = 1_800_000_000_000;
const SECRET: &str = "SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN";

fn window(percent: Value, duration: Value, reset: Value) -> Value {
    json!({"usedPercent":percent,"windowDurationMins":duration,"resetsAt":reset})
}
fn bucket() -> Value {
    json!({"limitId":"codex", "primary":window(json!(42),json!(10080),json!(1800000100)),
        "secondary":null,"accountId":SECRET,
        "credits":{"balance":SECRET},"upsell":SECRET})
}
fn flat(bucket: Value) -> Value {
    json!({"rateLimits":bucket, "rateLimitsByLimitId":null,"ordinaryUsageAllowed":true,"accountId":SECRET})
}
fn mapped(bucket: Value) -> Value {
    json!({"rateLimits":{"limitId":"foreign"},"rateLimitsByLimitId":{"codex":bucket},"ordinaryUsageAllowed":true})
}
fn unavailable(value: Value) {
    let read = normalize(value, NOW).unwrap();
    assert_eq!(read.snapshot.status, ProviderStatus::Unavailable);
    assert_eq!(read.snapshot.fetched_at, Some(NOW));
    assert_eq!(read.snapshot.remaining(NOW), None);
}

#[test]
fn weekly_only_primary_is_weekly_for_flat_and_map() {
    for value in [flat(bucket()), mapped(bucket())] {
        let read = normalize(value, NOW).unwrap();
        assert_eq!(read.snapshot.id, ProviderId::Codex);
        assert_eq!(read.snapshot.status, ProviderStatus::Current);
        assert_eq!(read.snapshot.windows.len(), 1);
        let w = &read.snapshot.windows[0];
        assert_eq!(w.id, "codex:primary");
        assert_eq!(w.label, "Неделя");
        assert_eq!(w.duration_mins, 10080);
        assert_eq!(w.resets_at, Some(1_800_000_100_000));
        assert_eq!(w.used_percent, 42.0);
        assert!(w.primary);
        assert_eq!(read.snapshot.remaining(NOW), Some(58.0));
        assert_eq!(read.ordinary_usage_allowed, Some(true));
    }
}

#[test]
fn authoritative_map_never_falls_back_to_legacy() {
    for map in [json!({}), json!({"review":bucket()})] {
        unavailable(json!({"rateLimits":bucket(),"rateLimitsByLimitId":map}));
    }
    let mut b = bucket();
    b["limitId"] = json!("review");
    unavailable(flat(b.clone()));
    unavailable(mapped(b));
}

#[test]
fn legacy_missing_limit_id_and_missing_map_are_accepted() {
    let mut b = bucket();
    b.as_object_mut().unwrap().remove("limitId");
    let read = normalize(json!({"rateLimits":b}), NOW).unwrap();
    assert_eq!(read.snapshot.windows.len(), 1);
}

#[test]
fn null_and_missing_windows_are_unavailable() {
    for b in [json!({}), json!({"primary":null,"secondary":null})] {
        unavailable(flat(b));
    }
}

#[test]
fn labels_and_minimum_use_actual_duration_not_slots() {
    let mut b = bucket();
    b["primary"] = window(json!(4), json!(90), Value::Null);
    b["secondary"] = window(json!(70), json!(300), Value::Null);
    let read = normalize(flat(b), NOW).unwrap();
    assert_eq!(read.snapshot.windows[0].label, "90 минут");
    assert_eq!(read.snapshot.windows[1].label, "5 часов");
    assert_eq!(read.snapshot.windows[1].id, "codex:secondary");
    assert!(read.snapshot.windows[1].primary);
    assert_eq!(read.snapshot.remaining(NOW), Some(30.0));
    for (duration, label) in [(120, "2 часов"), (17, "17 минут")] {
        let mut b = bucket();
        b["primary"]["windowDurationMins"] = json!(duration);
        assert_eq!(
            normalize(flat(b), NOW).unwrap().snapshot.windows[0].label,
            label
        );
    }
}

#[test]
fn unknown_duration_and_reset_do_not_invent_values() {
    for w in [
        window(json!(0), Value::Null, Value::Null),
        json!({"usedPercent":0}),
    ] {
        let read = normalize(flat(json!({"secondary":w})), NOW).unwrap();
        let w = &read.snapshot.windows[0];
        assert_eq!(w.duration_mins, 0);
        assert_eq!(w.label, "Неизвестная длительность");
        assert_eq!(w.resets_at, None);
        assert_eq!(w.id, "codex:secondary");
        assert_eq!(read.snapshot.remaining(NOW), Some(100.0));
    }
}

#[test]
fn expiry_and_freshness_are_left_to_existing_snapshot() {
    let mut b = bucket();
    b["primary"]["resetsAt"] = json!(1800000000);
    let read = normalize(flat(b), NOW).unwrap();
    assert_eq!(read.snapshot.status, ProviderStatus::Current);
    assert_eq!(read.snapshot.windows[0].resets_at, Some(NOW));
    assert_eq!(read.snapshot.remaining(NOW), None);
    let read = normalize(flat(bucket()), NOW).unwrap();
    assert_eq!(read.snapshot.remaining(NOW + STALE_AFTER_MS + 1), None);
}

#[test]
fn blocked_and_unknown_permission_remain_distinct() {
    for mut response in [flat(bucket()), mapped(bucket())] {
        response["ordinaryUsageAllowed"] = json!(false);
        let read = normalize(response, NOW).unwrap();
        assert_eq!(read.ordinary_usage_allowed, Some(false));
        assert_eq!(read.snapshot.windows.len(), 1);
        assert_eq!(read.snapshot.status, ProviderStatus::Unavailable);
        assert_eq!(read.snapshot.remaining(NOW), None);
    }
    for permission in [None, Some(Value::Null)] {
        let mut response = mapped(bucket());
        response
            .as_object_mut()
            .unwrap()
            .remove("ordinaryUsageAllowed");
        if let Some(p) = permission {
            response["ordinaryUsageAllowed"] = p;
        }
        let read = normalize(response, NOW).unwrap();
        assert_eq!(read.ordinary_usage_allowed, None);
        assert_eq!(read.snapshot.status, ProviderStatus::Current);
    }
}

#[test]
fn nested_permission_cannot_override_response_permission() {
    let mut nested = bucket();
    nested["ordinaryUsageAllowed"] = json!(false);
    let read = normalize(mapped(nested), NOW).unwrap();
    assert_eq!(read.ordinary_usage_allowed, Some(true));
    assert_eq!(read.snapshot.status, ProviderStatus::Current);

    let mut nested = bucket();
    nested["ordinaryUsageAllowed"] = json!(true);
    let mut response = mapped(nested);
    response["ordinaryUsageAllowed"] = json!(false);
    let read = normalize(response, NOW).unwrap();
    assert_eq!(read.ordinary_usage_allowed, Some(false));
    assert_eq!(read.snapshot.remaining(NOW), None);
}

#[test]
fn invalid_quota_numbers_are_rejected_not_clamped() {
    for (field, values) in [
        (
            "usedPercent",
            vec![
                json!(-1),
                json!(101),
                json!(1.5),
                json!(2147483648_i64),
                Value::Null,
                json!("42"),
            ],
        ),
        (
            "windowDurationMins",
            vec![json!(0), json!(-1), json!(4294967296_i64), json!(1.5)],
        ),
        (
            "resetsAt",
            vec![
                json!(-1),
                json!(i64::MAX),
                json!(1.5),
                json!(u64::MAX),
                json!(9007199254741_i64),
            ],
        ),
    ] {
        for value in values {
            let mut b = bucket();
            b["primary"][field] = value;
            assert_eq!(
                normalize(flat(b), NOW).unwrap_err(),
                CodexError::InvalidResponse
            );
        }
    }
    let mut b = bucket();
    b["primary"].as_object_mut().unwrap().remove("usedPercent");
    assert_eq!(
        normalize(flat(b), NOW).unwrap_err(),
        CodexError::InvalidResponse
    );
}

#[test]
fn required_response_shape_is_validated() {
    for value in [
        json!({}),
        json!({"rateLimits":null}),
        json!({"rateLimits":[]}),
        json!({"rateLimits":{},"rateLimitsByLimitId":[]}),
        flat(json!({"primary":"secret"})),
        json!({"rateLimits":{},"ordinaryUsageAllowed":"secret"}),
    ] {
        assert_eq!(
            normalize(value, NOW).unwrap_err(),
            CodexError::InvalidResponse
        );
    }
}

#[test]
fn sanitized_dto_serialization_and_debug_exclude_private_fields() {
    let read = normalize(mapped(bucket()), NOW).unwrap();
    let serialized = serde_json::to_value(&read).unwrap();
    assert_eq!(serialized["ordinaryUsageAllowed"], true);
    assert!(serialized.get("ordinary_usage_allowed").is_none());
    for text in [serialized.to_string(), format!("{read:?}")] {
        assert!(!text.contains(SECRET));
        for field in ["accountId", "email", "credits", "upsell"] {
            assert!(!text.contains(field));
        }
    }
}

// Synthetic scripts only. Python parses packets to verify the exact handshake and
// per-process arguments without depending on the installed Codex or its config.
pub(crate) struct Fake {
    pub(crate) dir: PathBuf,
    pub(crate) executable: PathBuf,
}
impl Fake {
    pub(crate) fn new(mode: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "kyl-codex-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let executable = dir.join("synthetic-server");
        let header = format!(
            "#!/usr/bin/python3\nMODE = {mode:?}\nDIRECTORY = {:?}\n",
            dir.to_str().unwrap()
        );
        fs::write(&executable, header + SCRIPT).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        Self { dir, executable }
    }
    pub(crate) async fn pids(&self) -> Vec<i32> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(text) = fs::read_to_string(self.dir.join("ready")) {
                return text
                    .split_whitespace()
                    .map(|v| v.parse().unwrap())
                    .collect();
            }
            assert!(Instant::now() < deadline, "synthetic server did not start");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    pub(crate) async fn cleaned(&self) {
        let pids = self.pids().await;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            // Each PID is freshly recorded by this fixture, never searched by name.
            if pids.iter().all(|&pid| unsafe { libc::kill(pid, 0) } == -1) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "owned synthetic process was not reaped"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

const SCRIPT: &str = r#"
import json, os, sys, time, tempfile
expected = []
for value in ['analytics.enabled=false','otel.exporter="none"','otel.trace_exporter="none"','otel.metrics_exporter="none"','features.plugins=false','features.remote_plugin=false','features.hooks=false','features.apps=false']:
    expected += ['-c',value]
assert sys.argv[1:] == expected + ['app-server','--listen','stdio://']
assert os.path.realpath(os.getcwd()) == os.path.realpath(tempfile.gettempdir())
assert os.getpgrp() == os.getpid()
with open(os.path.join(DIRECTORY,'launches'),'a') as f: f.write('1\n')
pids = [os.getpid()]
if MODE in ['group_success','group_error','cancel_group','timeout_group']:
    # Fake npm launcher plus a Rust-like helper in the same owned process group.
    helper = os.fork()
    if helper == 0:
        while True: time.sleep(1)
    pids.append(helper)
    # Reap the helper automatically when the whole group is killed (no zombie).
    import signal
    signal.signal(signal.SIGCHLD, signal.SIG_IGN)
with open(os.path.join(DIRECTORY,'child-path'),'w') as f: f.write(os.environ.get('PATH',''))
with open(os.path.join(DIRECTORY,'ready.tmp'),'w') as f: f.write(' '.join(map(str,pids)))
os.rename(os.path.join(DIRECTORY,'ready.tmp'),os.path.join(DIRECTORY,'ready'))
def send(value):
    print(json.dumps(value), flush=True)
def receive(method, ident=None):
    value = json.loads(sys.stdin.readline())
    assert value['method'] == method
    if ident is None: assert 'id' not in value
    else: assert value['id'] == ident
    return value
if MODE in ['cancel','cancel_group','timeout_group','blocked']:
    while True: time.sleep(1)
if MODE == 'eof': sys.exit(0)
if MODE == 'invalid': print('SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN',flush=True)
elif MODE == 'oversize':
    sys.stdout.write('x'*65537); sys.stdout.flush()
elif MODE == 'stderr':
    sys.stderr.write('x'*262145); sys.stderr.flush()
elif MODE == 'stdin_closed':
    os.close(0)
    send({'id':1,'result':{}})
else:
    receive('initialize',1)
    if MODE == 'request': send({'id':90,'method':'private/server/request','params':{'secret':'SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN'}})
    elif MODE == 'both': send({'id':1,'result':{},'error':{'code':-1,'message':'secret'}})
    elif MODE == 'neither': send({'id':1})
    elif MODE == 'string_id': send({'id':'1','result':{}})
    elif MODE == 'wrong_id': send({'id':999,'result':{}}); sys.exit(0)
    elif MODE == 'frames':
        for i in range(101): send({'method':'ignored','params':{}})
    elif MODE == 'total':
        for i in range(4): send({'method':'ignored','params':'x'*40000})
    elif MODE == 'rpc': send({'id':1,'error':{'code':-32001,'message':'SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN','data':{'accountId':'SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN'}}})
    elif MODE == 'rpc_bad_code': send({'id':1,'error':{'code':'SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN'}})
    else:
        if MODE == 'unexpected':
            send({'method':'unknown/notification','params':{'accountId':'SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN'}})
            send({'id':999,'result':{}})
        send({'id':1,'result':{'userAgent':'SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN'}})
        receive('initialized')
        receive('account/rateLimits/read',2)
        if MODE == 'held':
            while not os.path.exists(os.path.join(DIRECTORY,'release')): time.sleep(0.01)
        if MODE == 'group_error': print('invalid-json',flush=True)
        else:
            if MODE == 'stderr_ok': sys.stderr.write('x'*250000); sys.stderr.flush()
            quota = {'primary':{'usedPercent':42,'windowDurationMins':10080,'resetsAt':int(time.time())+86400},'accountId':'SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN'}
            if MODE == 'no_quota': quota = {}
            if MODE == 'expired': quota['primary']['resetsAt'] = 1
            send({'id':2,'result':{'ordinaryUsageAllowed':MODE != 'permission_false','rateLimits':quota,'rateLimitsByLimitId':None}})
            # No fourth packet allowed, even during cleanup.
            assert sys.stdin.readline() == ''
while True: time.sleep(1)
"#;

#[tokio::test(flavor = "current_thread")]
async fn normal_handshake_and_cleanup() {
    let fake = Fake::new("normal");
    let read = read(&fake.executable).await.unwrap();
    assert_eq!(read.snapshot.windows[0].duration_mins, 10080);
    assert!(!serde_json::to_string(&read).unwrap().contains(SECRET));
    fake.cleaned().await;
}

#[tokio::test(flavor = "current_thread")]
async fn bounded_unexpected_frames_are_ignored() {
    let fake = Fake::new("unexpected");
    assert!(read(&fake.executable).await.is_ok());
    fake.cleaned().await;
}

#[tokio::test(flavor = "current_thread")]
async fn protocol_failures_are_sanitized_and_reaped() {
    for mode in [
        "eof",
        "invalid",
        "wrong_id",
        "string_id",
        "request",
        "both",
        "neither",
        "rpc_bad_code",
    ] {
        let fake = Fake::new(mode);
        let error = read(&fake.executable).await.unwrap_err();
        assert_eq!(error, CodexError::Protocol, "mode {mode}");
        assert!(!format!("{error:?} {error}").contains(SECRET));
        fake.cleaned().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn rpc_error_carries_only_safe_code() {
    let fake = Fake::new("rpc");
    let error = read(&fake.executable).await.unwrap_err();
    assert_eq!(error, CodexError::Rpc(-32001));
    assert_eq!(error.to_string(), "Codex RPC failed");
    assert!(!format!("{error:?}").contains(SECRET));
    fake.cleaned().await;
}

#[tokio::test(flavor = "current_thread")]
async fn resource_budgets_bound_stdout_stderr_frames_and_total() {
    for mode in ["oversize", "stderr", "frames", "total"] {
        let fake = Fake::new(mode);
        assert_eq!(
            read(&fake.executable).await.unwrap_err(),
            CodexError::ResourceLimit,
            "mode {mode}"
        );
        fake.cleaned().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn large_but_bounded_stderr_is_drained_concurrently() {
    let fake = Fake::new("stderr_ok");
    assert!(read(&fake.executable).await.is_ok());
    fake.cleaned().await;
}

#[tokio::test(flavor = "current_thread")]
async fn spawn_and_write_failure_are_sanitized() {
    assert_eq!(
        read(Path::new("/nonexistent/SYNTHETIC_PRIVATE_ID_EMAIL_TOKEN"))
            .await
            .unwrap_err(),
        CodexError::Io
    );
    let fake = Fake::new("stdin_closed");
    assert!(matches!(
        read(&fake.executable).await.unwrap_err(),
        CodexError::Io | CodexError::Protocol
    ));
    fake.cleaned().await;
}

#[tokio::test(flavor = "current_thread")]
async fn timeout_covers_entire_exchange_and_reaps_group() {
    let fake = Fake::new("timeout_group");
    let start = Instant::now();
    assert_eq!(
        read(&fake.executable).await.unwrap_err(),
        CodexError::Timeout
    );
    assert!(start.elapsed() >= Duration::from_secs(15));
    assert!(start.elapsed() < Duration::from_secs(17));
    fake.cleaned().await;
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_reaps_launcher_and_helper() {
    let fake = Fake::new("cancel_group");
    let executable = fake.executable.clone();
    let task = tokio::spawn(async move { read(&executable).await });
    fake.pids().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    fake.cleaned().await;
}

#[tokio::test(flavor = "current_thread")]
async fn managed_cancel_awaits_cleanup_and_sets_only_child_path() {
    let fake = Fake::new("cancel_group");
    let executable = fake.executable.clone();
    let original_path = std::env::var_os("PATH");
    let original_home = std::env::var_os("CODEX_HOME");
    let (cancel, receiver) = tauri::async_runtime::channel(1);
    let task = tokio::spawn(async move {
        read_managed(&executable, std::ffi::OsStr::new("/usr/bin:/bin"), receiver).await
    });
    fake.pids().await;
    cancel.try_send(()).unwrap();
    assert_eq!(task.await.unwrap().unwrap_err(), CodexError::Cancelled);
    fake.cleaned().await;
    assert_eq!(
        fs::read_to_string(fake.dir.join("child-path")).unwrap(),
        "/usr/bin:/bin"
    );
    assert_eq!(std::env::var_os("PATH"), original_path);
    assert_eq!(std::env::var_os("CODEX_HOME"), original_home);
}

#[tokio::test(flavor = "current_thread")]
async fn success_and_error_kill_owned_helper_descendants() {
    for mode in ["group_success", "group_error"] {
        let fake = Fake::new(mode);
        let result = read(&fake.executable).await;
        assert_eq!(result.is_ok(), mode == "group_success");
        fake.cleaned().await;
    }
}

#[test]
fn quota_schema_requires_objects_not_positional_arrays() {
    for value in [
        mapped(json!([null, null, null, null])),
        flat(json!({"primary":[42,10080,1800000100]})),
    ] {
        assert_eq!(
            normalize(value, NOW).unwrap_err(),
            CodexError::InvalidResponse
        );
    }
}

#[test]
fn js_safe_reset_boundary_and_duration_upper_bound() {
    let mut b = bucket();
    b["primary"]["resetsAt"] = json!(9007199254740_i64);
    b["primary"]["windowDurationMins"] = json!(u32::MAX);
    let read = normalize(flat(b), NOW).unwrap();
    assert_eq!(
        read.snapshot.windows[0].resets_at,
        Some(9_007_199_254_740_000)
    );
    assert_eq!(read.snapshot.windows[0].duration_mins, u32::MAX);
    let mut b = bucket();
    b["primary"]["resetsAt"] = json!(0);
    assert_eq!(
        normalize(flat(b), NOW).unwrap().snapshot.windows[0].resets_at,
        Some(0)
    );
}

#[test]
fn only_selected_bucket_is_normalized() {
    let mut value = mapped(bucket());
    value["rateLimits"]["primary"] = json!({"usedPercent":-1000});
    value["rateLimitsByLimitId"]["review"] = json!({"primary":{"usedPercent":SECRET}});
    assert_eq!(normalize(value, NOW).unwrap().snapshot.windows.len(), 1);
}

struct DelayedExit {
    confirmed: std::sync::Arc<tokio::sync::Notify>,
    attempts: usize,
    errors_left: usize,
}
impl ExitWait for DelayedExit {
    async fn wait_for_exit(&mut self) -> std::io::Result<()> {
        self.attempts += 1;
        if self.errors_left > 0 {
            self.errors_left -= 1;
            return Err(std::io::Error::other("synthetic OS wait error"));
        }
        self.confirmed.notified().await;
        Ok(())
    }
}

#[tokio::test(flavor = "current_thread")]
async fn managed_cleanup_owns_wait_beyond_standalone_deadline_until_confirmed_exit() {
    let confirmed = std::sync::Arc::new(tokio::sync::Notify::new());
    let mut child = DelayedExit {
        confirmed: confirmed.clone(),
        attempts: 0,
        errors_left: 0,
    };
    let wait = wait_for_exit(&mut child, true);
    tokio::pin!(wait);
    // Poll this same cleanup future past the legacy cutoff, without aborting or
    // spawning a detached task. It must remain pending until OS confirmation.
    tokio::select! {
        result = &mut wait => panic!("managed cleanup returned before exit confirmation: {result:?}"),
        _ = tokio::time::sleep(CLEANUP_TIMEOUT + Duration::from_millis(100)) => {}
    }
    confirmed.notify_one();
    assert_eq!(wait.await, Ok(()));
}

#[tokio::test(flavor = "current_thread")]
async fn managed_cleanup_retries_same_wait_with_backoff_until_confirmed_exit() {
    let mut child = DelayedExit {
        confirmed: std::sync::Arc::new(tokio::sync::Notify::new()),
        attempts: 0,
        errors_left: 2,
    };
    // Even an exited child can have transient OS wait failures; only a successful
    // wait is confirmation, not an error or a retry deadline.
    child.confirmed.notify_one();
    let start = Instant::now();
    assert_eq!(
        timeout(Duration::from_secs(1), wait_for_exit(&mut child, true)).await,
        Ok(Ok(()))
    );
    assert_eq!(child.attempts, 3);
    assert!(start.elapsed() >= Duration::from_millis(100));
}

#[tokio::test(flavor = "current_thread")]
async fn standalone_cleanup_retains_bounded_deadline_and_immediate_wait_error() {
    let mut child = DelayedExit {
        confirmed: std::sync::Arc::new(tokio::sync::Notify::new()),
        attempts: 0,
        errors_left: 0,
    };
    let start = Instant::now();
    assert_eq!(
        wait_for_exit(&mut child, false).await,
        Err(CodexError::Cleanup)
    );
    assert!(start.elapsed() >= CLEANUP_TIMEOUT);
    assert_eq!(child.attempts, 1);
    // The bounded policy never invents confirmation or consumes the child.
    child.confirmed.notify_one();
    assert_eq!(wait_for_exit(&mut child, false).await, Ok(()));
    child.errors_left = 1;
    assert_eq!(
        timeout(Duration::from_millis(100), wait_for_exit(&mut child, false)).await,
        Ok(Err(CodexError::Cleanup))
    );
    assert_eq!(child.attempts, 3);
}
