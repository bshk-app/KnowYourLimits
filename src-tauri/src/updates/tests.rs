use super::*;
use crate::model::{AppState, Settings};
use std::sync::atomic::AtomicUsize;

struct FakeArtifact {
    download_error: Option<Notice>,
    install_error: Option<Notice>,
    downloads: AtomicUsize,
    installs: AtomicUsize,
    hold_download: AtomicBool,
    started: tokio::sync::Notify,
}
impl UpdateArtifact for FakeArtifact {
    fn version(&self) -> &str {
        "0.2.8"
    }
    fn download(&self, progress: Progress) -> Request<Vec<u8>> {
        self.downloads.fetch_add(1, Ordering::SeqCst);
        self.started.notify_one();
        let error = self.download_error;
        let hold = self.hold_download.load(Ordering::SeqCst);
        Box::pin(async move {
            progress(10, Some(10));
            if hold {
                std::future::pending::<()>().await;
            }
            match error {
                Some(e) => Err(e),
                None => Ok(vec![1, 2, 3]),
            }
        })
    }
    fn install(&self, _bytes: Vec<u8>) -> Request<()> {
        self.installs.fetch_add(1, Ordering::SeqCst);
        let error = self.install_error;
        Box::pin(async move { error.map_or(Ok(()), Err) })
    }
}
struct FakeSource {
    artifact: Option<Artifact>,
    error: Option<Notice>,
    checks: AtomicUsize,
    gate: Arc<tokio::sync::Notify>,
    hold: bool,
}
impl Source for FakeSource {
    fn check(&self) -> Request<Option<Artifact>> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        let artifact = self.artifact.clone();
        let error = self.error;
        let gate = self.gate.clone();
        let hold = self.hold;
        Box::pin(async move {
            if hold {
                gate.notified().await;
            }
            error.map_or(Ok(artifact), Err)
        })
    }
}
fn artifact(
    download_error: Option<Notice>,
    install_error: Option<Notice>,
    hold: bool,
) -> Arc<FakeArtifact> {
    Arc::new(FakeArtifact {
        download_error,
        install_error,
        downloads: AtomicUsize::new(0),
        installs: AtomicUsize::new(0),
        hold_download: AtomicBool::new(hold),
        started: tokio::sync::Notify::new(),
    })
}
fn source(artifact: Option<Artifact>, error: Option<Notice>, hold: bool) -> Arc<FakeSource> {
    Arc::new(FakeSource {
        artifact,
        error,
        checks: AtomicUsize::new(0),
        gate: Arc::new(tokio::sync::Notify::new()),
        hold,
    })
}
fn service(source: Arc<dyn Source>, installation: Installation) -> Updates {
    Updates::with_source(source, true, installation, |_| {})
}
async fn settled(service: &Updates) {
    let _transition = service.0.transition.lock().await;
    service.reap().await;
}
fn coordinator() -> Coordinator {
    Coordinator::new(AppState::disconnected(Settings::default()), |_| {})
}

#[tokio::test]
async fn startup_failure_is_silent_but_a_joining_manual_check_receives_the_error() {
    let backend = source(None, Some(Notice::CheckFailed), true);
    let updates = service(backend.clone(), Installation::Managed);
    updates.check(false).await.unwrap();
    tokio::task::yield_now().await;
    updates.check(true).await.unwrap();
    backend.gate.notify_one();
    settled(&updates).await;
    assert_eq!(backend.checks.load(Ordering::SeqCst), 1);
    assert_eq!(updates.snapshot().notice, Some(Notice::CheckFailed));
    let backend = source(None, Some(Notice::NotConfigured), false);
    let silent = service(backend, Installation::Managed);
    silent.check(false).await.unwrap();
    settled(&silent).await;
    assert_eq!(silent.snapshot().status, Status::Idle);
    assert_eq!(silent.snapshot().notice, None);
}
#[tokio::test]
async fn no_update_and_download_verification_failure_never_install() {
    let empty = service(source(None, None, false), Installation::Managed);
    empty.check(true).await.unwrap();
    settled(&empty).await;
    assert_eq!(empty.snapshot().status, Status::UpToDate);
    assert_eq!(empty.download().await.unwrap_err(), Notice::NoUpdate);
    let artifact = artifact(Some(Notice::VerificationFailed), None, false);
    let updates = service(
        source(Some(artifact.clone()), None, false),
        Installation::Managed,
    );
    updates.check(true).await.unwrap();
    settled(&updates).await;
    updates.download().await.unwrap();
    settled(&updates).await;
    assert_eq!(updates.snapshot().status, Status::Available);
    assert_eq!(updates.snapshot().notice, Some(Notice::VerificationFailed));
    assert_eq!(
        updates
            .install(true, coordinator(), || async {})
            .await
            .unwrap_err(),
        Notice::NoUpdate
    );
    assert_eq!(artifact.installs.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn duplicate_downloads_are_coalesced_and_cancel_waits_for_the_owned_request() {
    let artifact = artifact(None, None, true);
    let updates = service(
        source(Some(artifact.clone()), None, false),
        Installation::Managed,
    );
    updates.check(true).await.unwrap();
    settled(&updates).await;
    updates.download().await.unwrap();
    artifact.started.notified().await;
    updates.download().await.unwrap();
    assert_eq!(artifact.downloads.load(Ordering::SeqCst), 1);
    let state = updates.cancel_download().await;
    assert_eq!(state.status, Status::Available);
    assert_eq!(state.notice, Some(Notice::DownloadCancelled));
    assert!(updates.0.inner.lock().unwrap().job.is_none());
    assert!(updates.0.inner.lock().unwrap().bytes.is_none());
    artifact.hold_download.store(false, Ordering::SeqCst);
    updates.download().await.unwrap();
    settled(&updates).await;
    assert_eq!(updates.snapshot().status, Status::Ready);
}
#[tokio::test]
async fn installer_failure_does_not_restart_or_stop_the_coordinator() {
    let artifact = artifact(None, Some(Notice::InstallFailed), false);
    let updates = service(
        source(Some(artifact.clone()), None, false),
        Installation::Managed,
    );
    updates.check(true).await.unwrap();
    settled(&updates).await;
    updates.download().await.unwrap();
    settled(&updates).await;
    let coordinator = coordinator();
    let restarted = Arc::new(AtomicBool::new(false));
    let flag = restarted.clone();
    assert_eq!(
        updates
            .install(false, coordinator.clone(), || async {})
            .await
            .unwrap_err(),
        Notice::ConfirmationRequired
    );
    assert_eq!(artifact.installs.load(Ordering::SeqCst), 0);
    updates
        .install(true, coordinator.clone(), move || async move {
            flag.store(true, Ordering::SeqCst);
        })
        .await
        .unwrap();
    settled(&updates).await;
    assert_eq!(updates.snapshot().status, Status::Ready);
    assert_eq!(updates.snapshot().notice, Some(Notice::InstallFailed));
    assert!(!restarted.load(Ordering::SeqCst));
    assert!(!coordinator.is_stopping());
    // Reacquiring proves the first suspension was released after the failed installer.
    drop(coordinator.suspend_for_update().await.unwrap());
}
#[tokio::test]
async fn portable_and_development_builds_cannot_download_or_install() {
    for installation in [Installation::Manual, Installation::Development] {
        let artifact = artifact(None, None, false);
        let updates = service(source(Some(artifact.clone()), None, false), installation);
        updates.check(true).await.unwrap();
        settled(&updates).await;
        assert_eq!(updates.snapshot().status, Status::Available);
        assert_eq!(updates.download().await.unwrap_err(), Notice::ManualInstall);
        assert_eq!(
            updates
                .install(true, coordinator(), || async {})
                .await
                .unwrap_err(),
            Notice::ManualInstall
        );
        assert_eq!(artifact.downloads.load(Ordering::SeqCst), 0);
        assert_eq!(artifact.installs.load(Ordering::SeqCst), 0);
    }
}
#[tokio::test]
async fn shutdown_cancels_a_check_and_prevents_restart_claim_and_new_jobs() {
    let backend = source(None, None, true);
    let updates = service(backend, Installation::Managed);
    updates.check(false).await.unwrap();
    updates.shutdown().await;
    assert_eq!(updates.check(true).await.unwrap_err(), Notice::Stopping);
    assert!(!updates.prepare_restart());
    assert!(!updates.restart_ready());
    assert!(updates.0.inner.lock().unwrap().job.is_none());
}
#[tokio::test]
async fn unrecoverable_restore_failure_switches_to_manual_installation() {
    let artifact = artifact(None, Some(Notice::RestoreFailed), false);
    let updates = service(
        source(Some(artifact.clone()), None, false),
        Installation::Managed,
    );
    updates.check(true).await.unwrap();
    settled(&updates).await;
    updates.download().await.unwrap();
    settled(&updates).await;
    updates
        .install(true, coordinator(), || async {})
        .await
        .unwrap();
    settled(&updates).await;
    assert_eq!(updates.snapshot().installation, Installation::Manual);
    assert_eq!(updates.snapshot().notice, Some(Notice::RestoreFailed));
    assert_eq!(updates.download().await.unwrap_err(), Notice::ManualInstall);
}
