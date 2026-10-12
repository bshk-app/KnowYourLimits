use super::*;
use crate::model::QuotaWindow;
use std::sync::atomic::AtomicUsize;
fn sample(id: ProviderId, used: f64) -> ProviderSnapshot {
    ProviderSnapshot {
        id,
        label: id.label().into(),
        status: ProviderStatus::Current,
        fetched_at: Some(now_ms()),
        windows: vec![QuotaWindow {
            id: "weekly".into(),
            label: "Неделя".into(),
            used_percent: used,
            resets_at: None,
            duration_mins: 10080,
            primary: true,
        }],
    }
}
#[tokio::test(flavor = "current_thread")]
async fn last_quota_and_connection_survive_restart_without_running_a_cli() {
    let fake = crate::codex::tests::Fake::new("normal");
    let path = fake.dir.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let reader_calls = calls.clone();
    let reader: Reader = Arc::new(move |id, _, _| {
        reader_calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Ok(sample(id, 90.0)) })
    });
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        reader.clone(),
        Arc::new(|_| {}),
    )
    .with_persistence(path.clone());
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    let before = coordinator.wait_for_idle().await;
    coordinator.shutdown().await;
    let restarted = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        reader,
        Arc::new(|_| {}),
    )
    .with_persistence(path);
    let after = restarted.snapshot();
    assert_eq!(
        after.providers[1].windows.len(),
        1,
        "last quota disappeared after restart"
    );
    assert_eq!(after.providers[1].windows[0].used_percent, 90.0);
    assert_eq!(
        after.providers[1].fetched_at,
        before.providers[1].fetched_at
    );
    assert_eq!(after.connected, vec![ProviderId::Codex]);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "restart must not launch CLI"
    );
    restarted.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn restored_timer_resumes_and_a_failed_refresh_survives_another_restart() {
    let fake = crate::codex::tests::Fake::new("normal");
    let mut settings = Settings::default();
    settings.preferences.codex.refresh_minutes = 1;
    let first = Coordinator::with_reader(
        AppState::disconnected(settings.clone()),
        Arc::new(|id, _, _| Box::pin(async move { Ok(sample(id, 90.0)) })),
        Arc::new(|_| {}),
    )
    .with_persistence(fake.dir.clone());
    first.read_quota(ProviderId::Codex).await.unwrap();
    let before = first.wait_for_idle().await;
    first.shutdown().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let restarted = Coordinator::with_reader(
        AppState::disconnected(settings.clone()),
        Arc::new(move |id, _, _| {
            count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Err(read_failed(id)) })
        }),
        Arc::new(|_| {}),
    )
    .with_persistence(fake.dir.clone());
    restarted.refresh_due().await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    restarted.0.inner.lock().unwrap().last_attempt[1] =
        Some(Instant::now() - std::time::Duration::from_secs(61));
    restarted.refresh_due().await.unwrap();
    let failed = restarted.wait_for_idle().await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(failed.title(now_ms()), "C — · X ~10%");
    restarted.shutdown().await;
    let final_start = Coordinator::with_reader(
        AppState::disconnected(settings),
        Arc::new(|_, _, _| Box::pin(async { panic!("restart must not read CLI") })),
        Arc::new(|_| {}),
    )
    .with_persistence(fake.dir.clone());
    assert_eq!(
        final_start.snapshot().providers[1].fetched_at,
        before.providers[1].fetched_at
    );
    assert_eq!(final_start.snapshot().title(now_ms()), "C — · X ~10%");
    final_start.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn quota_save_failure_is_visible_and_keeps_the_live_observation() {
    let fake = crate::codex::tests::Fake::new("normal");
    std::fs::create_dir(fake.dir.join("quota-cache.json")).unwrap();
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        Arc::new(|id, _, _| Box::pin(async move { Ok(sample(id, 90.0)) })),
        Arc::new(|_| {}),
    )
    .with_persistence(fake.dir.clone());
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    let ready = coordinator.wait_for_idle().await;
    assert_eq!(ready.title(now_ms()), "C — · X 10%");
    assert!(ready
        .notice
        .as_deref()
        .unwrap()
        .contains("kyl:errorCacheSave"));
    assert!(!fake.dir.join("quota-cache.json.tmp").exists());
    coordinator.shutdown().await;
}
fn fixture(fake: &crate::codex::tests::Fake) -> Coordinator {
    let executable = fake.dir.join("codex");
    std::fs::copy(&fake.executable, &executable).unwrap();
    let dirs = vec![fake.dir.clone()];
    Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        Arc::new(move |id, sign_in, cancel| {
            let resolved = resolve_in(id, &dirs);
            Box::pin(async move { read_resolved(id, sign_in, resolved, cancel).await })
        }),
        Arc::new(|_| {}),
    )
}
#[tokio::test(flavor = "current_thread")]
async fn startup_never_executes_either_cli() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        Arc::new(move |id, _, _| {
            count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(sample(id, 23.0)) })
        }),
        Arc::new(|_| {}),
    );
    assert!(coordinator.snapshot().connected.is_empty());
    assert!(coordinator
        .snapshot()
        .providers
        .iter()
        .all(|p| p.windows.is_empty()));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    assert_eq!(
        coordinator.wait_for_idle().await.title(now_ms()),
        "C — · X 77%"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    coordinator.shutdown().await;
}
#[tokio::test(flavor = "current_thread")]
async fn connecting_claude_retains_codex_and_updates_both_counters() {
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        Arc::new(|id, _, _| {
            Box::pin(async move {
                Ok(sample(
                    id,
                    if id == ProviderId::Claude { 28.0 } else { 23.0 },
                ))
            })
        }),
        Arc::new(|_| {}),
    );
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    let codex = coordinator.wait_for_idle().await.providers[1].clone();
    let started = coordinator.read_quota(ProviderId::Claude).await.unwrap();
    assert_eq!(started.refreshing, Some(ProviderId::Claude));
    assert_eq!(started.providers[1].fetched_at, codex.fetched_at);
    assert_eq!(started.title(now_ms()), "C — · X 77%");
    let ready = coordinator.wait_for_idle().await;
    assert_eq!(ready.title(now_ms()), "C 72% · X 77%");
    assert_eq!(ready.connected.len(), 2);
    coordinator.shutdown().await;
}
#[tokio::test(flavor = "current_thread")]
async fn production_claude_path_uses_managed_reader_and_reports_signed_out() {
    for mode in ["normal", "signed_out"] {
        let fake = crate::claude::tests::Fake::new(mode);
        std::os::unix::fs::symlink(&fake.executable, fake.dir.join("claude")).unwrap();
        let dirs = vec![fake.dir.clone()];
        let coordinator = Coordinator::with_reader(
            AppState::disconnected(Settings::default()),
            Arc::new(move |id, sign_in, cancel| {
                let resolved = resolve_in(id, &dirs);
                Box::pin(async move { read_resolved(id, sign_in, resolved, cancel).await })
            }),
            Arc::new(|_| {}),
        );
        coordinator.read_quota(ProviderId::Claude).await.unwrap();
        let state = coordinator.wait_for_idle().await;
        if mode == "normal" {
            assert_eq!(state.title(now_ms()), "C 72% · X —");
            fake.cleaned("quota").await;
        } else {
            assert_eq!(state.providers[0].status, ProviderStatus::SignedOut);
            assert_eq!(state.notice.as_deref(), Some(CLAUDE_LOGIN_REQUIRED));
            fake.cleaned("auth").await;
        }
        assert!(!serde_json::to_string(&state)
            .unwrap()
            .contains("SYNTHETIC_PRIVATE"));
        coordinator.shutdown().await;
    }
}
#[tokio::test(flavor = "current_thread")]
async fn codex_handshake_is_owned_and_updates_only_requested_provider() {
    let fake = crate::codex::tests::Fake::new("normal");
    let coordinator = fixture(&fake);
    let initial = coordinator.read_quota(ProviderId::Codex).await.unwrap();
    assert_eq!(initial.refreshing, Some(ProviderId::Codex));
    assert_eq!(initial.revision, 1);
    let ready = coordinator.wait_for_idle().await;
    assert_eq!(ready.revision, 2);
    assert_eq!(ready.title(now_ms()), "C — · X 58%");
    assert!(ready.providers[0].windows.is_empty());
    fake.cleaned().await;
    coordinator.shutdown().await;
}
#[tokio::test(flavor = "current_thread")]
async fn duplicate_clicks_coalesce_cross_provider_waits_and_settings_survive_completion() {
    let fake = crate::codex::tests::Fake::new("held");
    let coordinator = fixture(&fake);
    let (a, b) = tokio::join!(
        coordinator.read_quota(ProviderId::Codex),
        coordinator.read_quota(ProviderId::Codex)
    );
    assert_eq!(a.unwrap().revision, b.unwrap().revision);
    fake.pids().await;
    assert!(coordinator.read_quota(ProviderId::Claude).await.is_err());
    let settings = Settings {
        bar_providers: vec![ProviderId::Codex],
        ..Settings::default()
    };
    coordinator
        .save_settings(settings.clone(), &fake.dir.join("settings.json"))
        .unwrap();
    std::fs::write(fake.dir.join("release"), "").unwrap();
    let state = coordinator.wait_for_idle().await;
    assert_eq!(state.revision, 3);
    assert_eq!(state.settings, settings);
    assert_eq!(
        std::fs::read_to_string(fake.dir.join("launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    fake.cleaned().await;
    coordinator.shutdown().await;
}
#[tokio::test(flavor = "current_thread")]
async fn failed_refresh_keeps_original_history_timestamp() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        Arc::new(move |id, _, _| {
            let success = count.fetch_add(1, Ordering::SeqCst) == 0;
            Box::pin(async move {
                if success {
                    Ok(sample(id, 42.0))
                } else {
                    Err(read_failed(id))
                }
            })
        }),
        Arc::new(|_| {}),
    );
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    let original = coordinator.wait_for_idle().await;
    let refreshing = coordinator.read_quota(ProviderId::Codex).await.unwrap();
    assert_eq!(refreshing.providers[1].status, ProviderStatus::Stale);
    assert_eq!(refreshing.title(now_ms()), "C — · X ~58%");
    let failed = coordinator.wait_for_idle().await;
    assert_eq!(
        failed.providers[1].fetched_at,
        original.providers[1].fetched_at
    );
    assert_eq!(failed.providers[1].windows[0].used_percent, 42.0);
    assert_eq!(failed.providers[1].status, ProviderStatus::Error);
    coordinator.shutdown().await;
}
#[tokio::test(flavor = "current_thread")]
async fn unavailable_expired_and_failure_never_show_zero() {
    for mode in ["permission_false", "expired", "no_quota", "rpc", "invalid"] {
        let fake = crate::codex::tests::Fake::new(mode);
        let coordinator = fixture(&fake);
        coordinator.read_quota(ProviderId::Codex).await.unwrap();
        let state = coordinator.wait_for_idle().await;
        assert_eq!(
            state.title(now_ms()),
            if mode == "expired" || mode == "permission_false" {
                "C — · X ~58%"
            } else {
                "C — · X —"
            }
        );
        assert!(state.notice.is_some());
        fake.cleaned().await;
        coordinator.shutdown().await;
    }
}
#[derive(Default)]
struct Controlled {
    started: tokio::sync::Notify,
    cancelled: tokio::sync::Notify,
    cleanup: tokio::sync::Notify,
    calls: AtomicUsize,
}
fn controlled() -> (Coordinator, Arc<Controlled>) {
    let control = Arc::new(Controlled::default());
    let r = control.clone();
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        Arc::new(move |id, _, mut cancel| {
            let c = r.clone();
            Box::pin(async move {
                c.calls.fetch_add(1, Ordering::SeqCst);
                c.started.notify_one();
                cancel.recv().await;
                c.cancelled.notify_one();
                c.cleanup.notified().await;
                Ok(sample(id, 96.0))
            })
        }),
        Arc::new(|_| {}),
    );
    (coordinator, control)
}
#[tokio::test(flavor = "current_thread")]
async fn cancellation_rejects_late_result_and_blocks_new_job_until_cleanup() {
    let (coordinator, control) = controlled();
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    control.started.notified().await;
    let returning = coordinator.clone();
    let returned = tokio::spawn(async move { returning.cancel_read().await });
    control.cancelled.notified().await;
    assert!(!returned.is_finished());
    let starting = coordinator.clone();
    let started = tokio::spawn(async move { starting.read_quota(ProviderId::Claude).await });
    tokio::task::yield_now().await;
    assert!(!started.is_finished());
    control.cleanup.notify_one();
    let cancelled = returned.await.unwrap().unwrap();
    assert!(cancelled.providers.iter().all(|p| p.windows.is_empty()));
    let state = started.await.unwrap().unwrap();
    assert!(state.revision > cancelled.revision);
    control.started.notified().await;
    control.cleanup.notify_one();
    coordinator.shutdown().await;
}
#[tokio::test(flavor = "current_thread")]
async fn shutdown_during_cancellation_awaits_same_owned_cleanup_and_blocks_new_jobs() {
    let (coordinator, control) = controlled();
    coordinator.sign_in_claude().await.unwrap();
    control.started.notified().await;
    let returning = coordinator.clone();
    let returned = tokio::spawn(async move { returning.cancel_read().await });
    control.cancelled.notified().await;
    let stopping = coordinator.clone();
    let shutdown = tokio::spawn(async move { stopping.shutdown().await });
    tokio::task::yield_now().await;
    assert!(!shutdown.is_finished());
    assert!(!returned.is_finished());
    control.cleanup.notify_one();
    assert!(returned.await.unwrap().is_err());
    shutdown.await.unwrap();
    assert!(coordinator.read_quota(ProviderId::Codex).await.is_err());
    assert!(coordinator.sign_in_claude().await.is_err());
    coordinator.shutdown().await;
    assert!(coordinator.snapshot().refreshing.is_none());
    assert_eq!(control.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test(flavor = "current_thread")]
async fn cancellation_and_shutdown_reap_real_process_group() {
    for shutdown in [false, true] {
        let fake = crate::codex::tests::Fake::new("cancel_group");
        let coordinator = fixture(&fake);
        coordinator.read_quota(ProviderId::Codex).await.unwrap();
        let pids = fake.pids().await;
        if shutdown {
            let ((), ()) = tokio::join!(coordinator.shutdown(), coordinator.shutdown());
        } else {
            coordinator.cancel_read().await.unwrap();
        }
        assert!(pids.iter().all(|&pid| unsafe { libc::kill(pid, 0) } == -1));
        assert!(coordinator.snapshot().refreshing.is_none());
        coordinator.shutdown().await;
    }
}
#[tokio::test(flavor = "current_thread")]
async fn callbacks_can_read_latest_state_without_deadlock() {
    let slot: Arc<Mutex<Option<Coordinator>>> = Arc::new(Mutex::new(None));
    let read_slot = slot.clone();
    let events = Arc::new(Mutex::new(vec![]));
    let ev = events.clone();
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        Arc::new(|id, _, _| Box::pin(async move { Err(missing_cli(id)) })),
        Arc::new(move |state| {
            assert!(
                read_slot
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .snapshot()
                    .revision
                    >= state.revision
            );
            ev.lock().unwrap().push(state.revision);
        }),
    );
    *slot.lock().unwrap() = Some(coordinator.clone());
    coordinator.read_quota(ProviderId::Claude).await.unwrap();
    coordinator.wait_for_idle().await;
    assert_eq!(*events.lock().unwrap(), vec![1, 2]);
    coordinator.shutdown().await;
    *slot.lock().unwrap() = None;
}
#[test]
fn resolver_uses_only_absolute_fixed_bins_and_rejects_missing_relative_nonexec_and_dirs() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let fake = crate::codex::tests::Fake::new("normal");
    assert!(resolve_in(ProviderId::Codex, &[std::path::PathBuf::from(".")]).is_err());
    assert!(resolve_in(ProviderId::Codex, std::slice::from_ref(&fake.dir)).is_err());
    let path = fake.dir.join("codex");
    std::fs::create_dir(&path).unwrap();
    assert!(resolve_in(ProviderId::Codex, std::slice::from_ref(&fake.dir)).is_err());
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, "").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(resolve_in(ProviderId::Codex, std::slice::from_ref(&fake.dir)).is_err());
    std::fs::remove_file(&path).unwrap();
    symlink(&fake.executable, &path).unwrap();
    let resolved = resolve_in(ProviderId::Codex, std::slice::from_ref(&fake.dir)).unwrap();
    assert_eq!(resolved.executable, path);
    assert_eq!(
        std::env::split_paths(&resolved.child_path).collect::<Vec<_>>(),
        vec![
            fake.dir.clone(),
            "/bin".into(),
            "/usr/sbin".into(),
            "/sbin".into()
        ]
    );
    let dirs = known_dirs(Some("/synthetic/home".into()));
    assert_eq!(
        dirs,
        [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/synthetic/home/.bun/bin",
            "/synthetic/home/.local/bin",
            "/synthetic/home/.npm-global/bin",
            "/synthetic/home/.volta/bin",
            "/usr/bin"
        ]
        .map(std::path::PathBuf::from)
    );
    assert_eq!(known_dirs(Some("relative-home".into())), known_dirs(None));
}

#[tokio::test(flavor = "current_thread")]
async fn finder_env_node_wrapper_uses_discovered_child_path_not_parent_environment() {
    use std::os::unix::fs::PermissionsExt;
    let fake = crate::codex::tests::Fake::new("normal");
    let coordinator = fixture(&fake);
    let original_path = std::env::var_os("PATH");
    let node = fake.dir.join("node");
    let source = std::fs::read_to_string(&fake.executable).unwrap();
    // Fake node verifies the same stdio handshake and CLI arguments after the
    // /usr/bin/env launcher supplies its script filename. No real Node/Codex.
    let source = source.replacen(
        "import json, os, sys, time, tempfile",
        "import json, os, sys, time, tempfile\nsys.argv.pop(1)",
        1,
    );
    std::fs::write(&node, source).unwrap();
    std::fs::set_permissions(&node, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(fake.dir.join("codex"), "#!/usr/bin/env node\n").unwrap();
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    let state = coordinator.wait_for_idle().await;
    assert_eq!(state.title(now_ms()), "C — · X 58%");
    assert_eq!(std::env::var_os("PATH"), original_path);
    let child_path = std::fs::read_to_string(fake.dir.join("child-path")).unwrap();
    assert_eq!(
        std::env::split_paths(&child_path).next(),
        Some(fake.dir.clone())
    );
    fake.cleaned().await;
    coordinator.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn timers_are_per_provider_session_opt_in_and_single_flight() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let mut settings = Settings::default();
    settings.preferences.claude.refresh_minutes = 1;
    settings.preferences.codex.refresh_minutes = 5;
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(settings),
        Arc::new(move |id, _, _| {
            count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(sample(id, 23.0)) })
        }),
        Arc::new(|_| {}),
    );
    coordinator.refresh_due().await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    coordinator.read_quota(ProviderId::Claude).await.unwrap();
    coordinator.wait_for_idle().await;
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    coordinator.wait_for_idle().await;
    coordinator.refresh_due().await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    {
        let mut inner = coordinator.0.inner.lock().unwrap();
        inner.last_attempt = [
            Some(Instant::now() - std::time::Duration::from_secs(61)),
            Some(Instant::now() - std::time::Duration::from_secs(361)),
        ];
    }
    let first = coordinator.refresh_due().await.unwrap();
    assert_eq!(first.refreshing, Some(ProviderId::Codex));
    // A simultaneous timer tick never queues a second CLI, even for another provider.
    assert_eq!(
        coordinator.refresh_due().await.unwrap().revision,
        first.revision
    );
    coordinator.wait_for_idle().await;
    let second = coordinator.refresh_due().await.unwrap();
    assert_eq!(second.refreshing, Some(ProviderId::Claude));
    coordinator.wait_for_idle().await;
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    coordinator.refresh_due().await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    coordinator.shutdown().await;
}
#[tokio::test(flavor = "current_thread")]
async fn enabling_timer_starts_a_new_interval_and_off_cancels_future_reads() {
    let fake = crate::codex::tests::Fake::new("normal");
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        Arc::new(move |id, _, _| {
            count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(sample(id, 23.0)) })
        }),
        Arc::new(|_| {}),
    );
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    coordinator.wait_for_idle().await;
    {
        coordinator.0.inner.lock().unwrap().last_attempt[1] =
            Some(Instant::now() - std::time::Duration::from_secs(7200));
    }
    let mut settings = Settings::default();
    settings.preferences.codex.refresh_minutes = 1;
    coordinator
        .save_settings(settings.clone(), &fake.dir.join("settings.json"))
        .unwrap();
    coordinator.refresh_due().await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    {
        coordinator.0.inner.lock().unwrap().last_attempt[1] =
            Some(Instant::now() - std::time::Duration::from_secs(61));
    }
    settings.preferences.codex.refresh_minutes = 0;
    coordinator
        .save_settings(settings, &fake.dir.join("settings.json"))
        .unwrap();
    coordinator.refresh_due().await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    coordinator.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn immediate_fifteen_minute_timer_is_saved_and_runs_without_changing_other_preferences() {
    let fake = crate::codex::tests::Fake::new("normal");
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let mut settings = Settings::default();
    settings.preferences.claude.notifications.enabled = true;
    settings.preferences.claude.color = "#d97757".into();
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(settings.clone()),
        Arc::new(move |id, _, _| {
            count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(sample(id, 23.0)) })
        }),
        Arc::new(|_| {}),
    )
    .with_persistence(fake.dir.clone());
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    coordinator.wait_for_idle().await;
    let path = fake.dir.join("settings.json");
    let applied = coordinator
        .set_refresh_minutes(ProviderId::Codex, 15, &path)
        .unwrap();
    assert_eq!(applied.settings.preferences.codex.refresh_minutes, 15);
    assert_eq!(
        applied.settings.preferences.claude.color,
        settings.preferences.claude.color
    );
    assert!(applied.settings.preferences.claude.notifications.enabled);
    assert_eq!(crate::storage::load(&path).0, applied.settings);
    coordinator.refresh_due().await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    coordinator.0.inner.lock().unwrap().last_attempt[1] =
        Some(Instant::now() - std::time::Duration::from_secs(899));
    coordinator.refresh_due().await.unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "15-minute timer fired too early"
    );
    coordinator.0.inner.lock().unwrap().last_attempt[1] =
        Some(Instant::now() - std::time::Duration::from_secs(901));
    coordinator.refresh_due().await.unwrap();
    coordinator.wait_for_idle().await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    coordinator.shutdown().await;
    let restarted = Coordinator::with_reader(
        AppState::disconnected(crate::storage::load(&path).0),
        Arc::new(|id, _, _| Box::pin(async move { Ok(sample(id, 23.0)) })),
        Arc::new(|_| {}),
    )
    .with_persistence(fake.dir.clone());
    assert_eq!(
        restarted
            .snapshot()
            .settings
            .preferences
            .codex
            .refresh_minutes,
        15
    );
    assert!(restarted.snapshot().connected.contains(&ProviderId::Codex));
    restarted.0.inner.lock().unwrap().last_attempt[1] =
        Some(Instant::now() - std::time::Duration::from_secs(901));
    assert_eq!(
        restarted.refresh_due().await.unwrap().refreshing,
        Some(ProviderId::Codex)
    );
    restarted.wait_for_idle().await;
    restarted.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_or_failed_timer_save_keeps_the_previous_schedule() {
    let fake = crate::codex::tests::Fake::new("normal");
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        Arc::new(|id, _, _| Box::pin(async move { Ok(sample(id, 23.0)) })),
        Arc::new(|_| {}),
    );
    let path = fake.dir.join("settings.json");
    coordinator
        .set_refresh_minutes(ProviderId::Codex, 15, &path)
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(coordinator
        .set_refresh_minutes(ProviderId::Codex, 3, &path)
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let blocked = fake.dir.join("blocked.json");
    std::fs::create_dir(&blocked).unwrap();
    assert!(coordinator
        .set_refresh_minutes(ProviderId::Codex, 30, &blocked)
        .is_err());
    assert_eq!(
        coordinator
            .snapshot()
            .settings
            .preferences
            .codex
            .refresh_minutes,
        15
    );
    coordinator.shutdown().await;
}
#[tokio::test(flavor = "current_thread")]
async fn automatic_failures_wait_for_interval_without_losing_history() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let mut settings = Settings::default();
    settings.preferences.codex.refresh_minutes = 1;
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(settings),
        Arc::new(move |id, _, _| {
            let first = count.fetch_add(1, Ordering::SeqCst) == 0;
            Box::pin(async move {
                if first {
                    Ok(sample(id, 85.0))
                } else {
                    Err(read_failed(id))
                }
            })
        }),
        Arc::new(|_| {}),
    );
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    let original = coordinator.wait_for_idle().await;
    {
        coordinator.0.inner.lock().unwrap().last_attempt[1] =
            Some(Instant::now() - std::time::Duration::from_secs(61));
    }
    coordinator.refresh_due().await.unwrap();
    let failed = coordinator.wait_for_idle().await;
    assert_eq!(
        failed.providers[1].fetched_at,
        original.providers[1].fetched_at
    );
    assert_eq!(failed.providers[1].status, ProviderStatus::Error);
    coordinator.refresh_due().await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    coordinator.shutdown().await;
}
#[tokio::test(flavor = "current_thread")]
async fn committed_quota_drives_notification_dedup_and_saved_preferences() {
    let fake = crate::codex::tests::Fake::new("normal");
    let events = Arc::new(Mutex::new(Vec::new()));
    let output = events.clone();
    let mut settings = Settings::default();
    settings.preferences.claude.notifications.enabled = true;
    let mut coordinator = Coordinator::with_reader(
        AppState::disconnected(settings),
        Arc::new(|id, _, _| Box::pin(async move { Ok(sample(id, 96.0)) })),
        Arc::new(|_| {}),
    );
    Arc::get_mut(&mut coordinator.0).unwrap().notify = Arc::new(move |alert| {
        output.lock().unwrap().push(alert);
    });
    for _ in 0..2 {
        coordinator.read_quota(ProviderId::Claude).await.unwrap();
        coordinator.wait_for_idle().await;
    }
    assert_eq!(events.lock().unwrap().len(), 1);
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    coordinator.wait_for_idle().await;
    assert_eq!(events.lock().unwrap().len(), 1);
    let mut preferences = coordinator.snapshot().settings;
    preferences.preferences.claude.notifications.sound = true;
    coordinator
        .save_settings(preferences, &fake.dir.join("settings.json"))
        .unwrap();
    coordinator.read_quota(ProviderId::Claude).await.unwrap();
    coordinator.wait_for_idle().await;
    assert_eq!(
        events.lock().unwrap().len(),
        1,
        "changing sound must not repeat an already delivered quota warning"
    );
    let mut settings = coordinator.snapshot().settings;
    settings.preferences.claude.notifications.enabled = false;
    coordinator
        .save_settings(settings, &fake.dir.join("settings.json"))
        .unwrap();
    coordinator.read_quota(ProviderId::Claude).await.unwrap();
    coordinator.wait_for_idle().await;
    assert_eq!(events.lock().unwrap().len(), 1);
    coordinator.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn rate_limit_retains_history_blocks_clicks_and_timer_but_allows_other_provider() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let mut settings = Settings::default();
    settings.preferences.claude.refresh_minutes = 1;
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(settings),
        Arc::new(move |id, _, _| {
            let n = count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                if n == 1 {
                    Err(ReadFailure::RateLimited {
                        retry_after_secs: 300,
                    })
                } else {
                    Ok(sample(id, 29.0))
                }
            })
        }),
        Arc::new(|_| {}),
    );
    coordinator.read_quota(ProviderId::Claude).await.unwrap();
    let original = coordinator.wait_for_idle().await;
    coordinator.read_quota(ProviderId::Claude).await.unwrap();
    let blocked = coordinator.wait_for_idle().await;
    assert_eq!(
        blocked.providers[0].fetched_at,
        original.providers[0].fetched_at
    );
    assert_eq!(blocked.providers[0].windows[0].used_percent, 29.0);
    assert_eq!(blocked.providers[0].status, ProviderStatus::Error);
    assert!(blocked.retry_at.claude.unwrap() > now_ms());
    assert_eq!(blocked.retry_at.codex, None);
    assert!(!blocked.notice.as_deref().unwrap().contains("подписк"));
    for _ in 0..3 {
        coordinator.read_quota(ProviderId::Claude).await.unwrap();
        coordinator.refresh_due().await.unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    coordinator.wait_for_idle().await;
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert!(coordinator.snapshot().retry_at.claude.is_some());
    {
        let mut inner = coordinator.0.inner.lock().unwrap();
        inner.blocked_until[0] = Some(Instant::now() - std::time::Duration::from_secs(1));
        inner.last_attempt[0] = Some(Instant::now() - std::time::Duration::from_secs(301));
    }
    coordinator.refresh_due().await.unwrap();
    let recovered = coordinator.wait_for_idle().await;
    assert_eq!(recovered.providers[0].status, ProviderStatus::Current);
    assert_eq!(recovered.retry_at.claude, None);
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    coordinator.shutdown().await;
}
#[tokio::test(flavor = "current_thread")]
async fn empty_success_never_erases_the_last_real_snapshot_or_renews_its_timestamp() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        Arc::new(move |id, _, _| {
            let first = count.fetch_add(1, Ordering::SeqCst) == 0;
            Box::pin(async move {
                let mut p = sample(id, 29.0);
                if !first {
                    p.status = ProviderStatus::Unavailable;
                    p.windows.clear();
                    p.fetched_at = None;
                }
                Ok(p)
            })
        }),
        Arc::new(|_| {}),
    );
    coordinator.read_quota(ProviderId::Claude).await.unwrap();
    let original = coordinator.wait_for_idle().await;
    coordinator.read_quota(ProviderId::Claude).await.unwrap();
    let missing = coordinator.wait_for_idle().await;
    assert_eq!(
        missing.providers[0].fetched_at,
        original.providers[0].fetched_at
    );
    assert_eq!(missing.providers[0].windows[0].used_percent, 29.0);
    assert_eq!(missing.providers[0].status, ProviderStatus::Stale);
    assert_eq!(missing.title(now_ms()), "C ~71% · X —");
    coordinator.shutdown().await;
}
#[tokio::test(flavor = "current_thread")]
async fn actual_claude_reader_maps_http_rate_limit_to_app_cooldown_without_subscription_hint() {
    let fake = crate::claude::tests::Fake::new("rate_limited");
    std::os::unix::fs::symlink(&fake.executable, fake.dir.join("claude")).unwrap();
    let dirs = vec![fake.dir.clone()];
    let coordinator = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        Arc::new(move |id, sign_in, cancel| {
            let resolved = resolve_in(id, &dirs);
            Box::pin(async move { read_resolved(id, sign_in, resolved, cancel).await })
        }),
        Arc::new(|_| {}),
    );
    coordinator.read_quota(ProviderId::Claude).await.unwrap();
    let state = coordinator.wait_for_idle().await;
    assert!(state.retry_at.claude.is_some());
    assert_eq!(state.providers[0].status, ProviderStatus::Error);
    assert!(state.providers[0].windows.is_empty());
    assert!(state.notice.as_deref().unwrap().contains("kyl:rateLimited"));
    assert!(!serde_json::to_string(&state)
        .unwrap()
        .contains("SYNTHETIC_PRIVATE"));
    fake.cleaned("quota").await;
    coordinator.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn quota_pause_survives_restarts_without_starting_cli_or_affecting_codex() {
    let fake = crate::codex::tests::Fake::new("normal");
    let path = fake.dir.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let first = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        Arc::new(move |_, _, _| {
            count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Err(ReadFailure::RateLimited {
                    retry_after_secs: 900,
                })
            })
        }),
        Arc::new(|_| {}),
    )
    .with_persistence(path.clone());
    first.read_quota(ProviderId::Claude).await.unwrap();
    let initial = first.wait_for_idle().await;
    first.shutdown().await;
    let count = calls.clone();
    let restarted = Coordinator::with_reader(
        AppState::disconnected(Settings::default()),
        Arc::new(move |id, _, _| {
            count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(sample(id, 29.0)) })
        }),
        Arc::new(|_| {}),
    )
    .with_persistence(path);
    assert_eq!(
        restarted.snapshot().retry_at.claude,
        initial.retry_at.claude
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    restarted.read_quota(ProviderId::Claude).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    restarted.read_quota(ProviderId::Codex).await.unwrap();
    restarted.wait_for_idle().await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(restarted.snapshot().retry_at.claude.is_some());
    restarted.shutdown().await;
}

fn counting(fail: bool) -> (Arc<AtomicUsize>, Reader) {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let reader: Reader = Arc::new(move |id, _, _| {
        count.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if fail {
                Err(read_failed(id))
            } else {
                Ok(sample(id, 40.0))
            }
        })
    });
    (calls, reader)
}
#[tokio::test(flavor = "current_thread")]
async fn recent_answer_is_reused_for_a_minute_and_a_failure_for_twenty_seconds() {
    for (fail, reuse) in [(false, REUSE_SUCCESS), (true, REUSE_FAILURE)] {
        let (calls, reader) = counting(fail);
        let coordinator = Coordinator::with_reader(
            AppState::disconnected(Settings::default()),
            reader,
            Arc::new(|_| {}),
        )
        .with_reuse(REUSE_SUCCESS, REUSE_FAILURE);
        coordinator.read_quota(ProviderId::Claude).await.unwrap();
        coordinator.wait_for_idle().await;
        let until = coordinator.0.inner.lock().unwrap().fresh_until[0].unwrap();
        let left = until.saturating_duration_since(Instant::now());
        assert!(left <= reuse && left > reuse - std::time::Duration::from_secs(5));
        coordinator.read_quota(ProviderId::Claude).await.unwrap();
        coordinator.wait_for_idle().await;
        assert_eq!(calls.load(Ordering::SeqCst), 1, "reused within the window");
        // Sign-in is an explicit user action and is never answered from reuse.
        coordinator.sign_in_claude().await.unwrap();
        coordinator.wait_for_idle().await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        coordinator.0.inner.lock().unwrap().fresh_until[0] = Some(Instant::now());
        coordinator.read_quota(ProviderId::Claude).await.unwrap();
        coordinator.wait_for_idle().await;
        assert_eq!(calls.load(Ordering::SeqCst), 3, "read again once expired");
        coordinator.shutdown().await;
    }
}
#[tokio::test(flavor = "current_thread")]
async fn auto_timer_polls_every_five_minutes_after_panel_use_else_fifteen() {
    let mut settings = Settings::default();
    settings.preferences.claude.refresh_minutes = crate::model::AUTO_REFRESH_MINUTES;
    let (_calls, reader) = counting(false);
    let coordinator =
        Coordinator::with_reader(AppState::disconnected(settings), reader, Arc::new(|_| {}));
    let minutes = |m: u64| std::time::Duration::from_secs(m * 60);
    {
        let mut inner = coordinator.0.inner.lock().unwrap();
        inner.state.connected.push(ProviderId::Claude);
        let now = Instant::now();
        inner.last_attempt[0] = Some(now - minutes(6));
        assert_eq!(due_provider(&inner, now), None, "idle: waits 15 min");
        inner.last_attempt[0] = Some(now - minutes(16));
        assert_eq!(due_provider(&inner, now), Some(ProviderId::Claude));
        inner.last_attempt[0] = Some(now - minutes(6));
    }
    coordinator.note_activity();
    assert_eq!(
        due_provider(&coordinator.0.inner.lock().unwrap(), Instant::now()),
        Some(ProviderId::Claude),
        "panel just used: 5 min suffice"
    );
    coordinator.0.inner.lock().unwrap().last_activity = Some(Instant::now() - minutes(31));
    assert_eq!(
        due_provider(&coordinator.0.inner.lock().unwrap(), Instant::now()),
        None,
        "panel unused for 30 min: back to 15"
    );
    coordinator.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn updater_suspension_waits_for_cleanup_and_resumes_only_after_guard_drop() {
    let (coordinator, control) = controlled();
    coordinator.read_quota(ProviderId::Codex).await.unwrap();
    control.started.notified().await;
    let pausing = coordinator.clone();
    let pause = tokio::spawn(async move { pausing.suspend_for_update().await });
    control.cancelled.notified().await;
    assert!(
        !pause.is_finished(),
        "installer must wait for owned CLI cleanup"
    );
    control.cleanup.notify_one();
    let guard = pause.await.unwrap().unwrap();
    assert!(coordinator.read_quota(ProviderId::Claude).await.is_err());
    assert!(coordinator.snapshot().refreshing.is_none());
    assert_eq!(control.calls.load(Ordering::SeqCst), 1);
    drop(guard);
    coordinator.read_quota(ProviderId::Claude).await.unwrap();
    control.started.notified().await;
    assert_eq!(control.calls.load(Ordering::SeqCst), 2);
    control.cleanup.notify_one();
    coordinator.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn dropping_updater_suspension_never_revives_real_shutdown() {
    let (coordinator, control) = controlled();
    let guard = coordinator.suspend_for_update().await.unwrap();
    coordinator.shutdown().await;
    drop(guard);
    assert!(coordinator.read_quota(ProviderId::Codex).await.is_err());
    assert_eq!(control.calls.load(Ordering::SeqCst), 0);
}
