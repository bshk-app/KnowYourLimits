//! App-owned updater state. No credentials, installer bytes, or URLs cross IPC.
use crate::live::Coordinator;
use serde::Serialize;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::AppHandle;
#[cfg(windows)]
use tauri::Manager;
use tauri_plugin_updater::UpdaterExt;
use tokio::{
    sync::{mpsc, Mutex as AsyncMutex},
    task::JoinHandle,
};

type Request<T> = Pin<Box<dyn Future<Output = Result<T, Notice>> + Send>>;
type Progress = Arc<dyn Fn(usize, Option<u64>) + Send + Sync>;
type Artifact = Arc<dyn UpdateArtifact>;
trait UpdateArtifact: Send + Sync {
    fn version(&self) -> &str;
    fn download(&self, progress: Progress) -> Request<Vec<u8>>;
    fn install(&self, bytes: Vec<u8>) -> Request<()>;
}
trait Source: Send + Sync {
    fn check(&self) -> Request<Option<Artifact>>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Ready,
    Installing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Installation {
    Managed,
    Manual,
    Development,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Notice {
    CheckFailed,
    NotConfigured,
    DownloadFailed,
    VerificationFailed,
    DownloadCancelled,
    InstallFailed,
    RestoreFailed,
    ConfirmationRequired,
    ManualInstall,
    NoUpdate,
    Stopping,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    pub revision: u64,
    pub status: Status,
    pub supported: bool,
    pub installation: Installation,
    pub version: Option<String>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub notice: Option<Notice>,
}
#[derive(Clone)]
pub struct Updates(Arc<Core>);
struct Core {
    inner: Mutex<Inner>,
    transition: AsyncMutex<()>,
    stopping: AtomicBool,
    restart_ready: AtomicBool,
    source: Arc<dyn Source>,
    publish: Arc<dyn Fn(UpdateState) + Send + Sync>,
}
struct Inner {
    state: UpdateState,
    generation: u64,
    active: bool,
    manual: bool,
    candidate: Option<Artifact>,
    bytes: Option<Vec<u8>>,
    job: Option<Job>,
}
struct Job {
    cancel: mpsc::Sender<()>,
    handle: JoinHandle<()>,
}

impl Updates {
    pub fn new(app: &AppHandle, publish: impl Fn(UpdateState) + Send + Sync + 'static) -> Self {
        Self::with_source(
            Arc::new(NativeSource(app.clone())),
            supported_platform(),
            installation(app),
            publish,
        )
    }
    fn with_source(
        source: Arc<dyn Source>,
        supported: bool,
        installation: Installation,
        publish: impl Fn(UpdateState) + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(Core {
            inner: Mutex::new(Inner {
                state: UpdateState {
                    revision: 0,
                    status: Status::Idle,
                    supported,
                    installation,
                    version: None,
                    downloaded: 0,
                    total: None,
                    notice: None,
                },
                generation: 0,
                active: false,
                manual: false,
                candidate: None,
                bytes: None,
                job: None,
            }),
            transition: AsyncMutex::new(()),
            stopping: AtomicBool::new(false),
            restart_ready: AtomicBool::new(false),
            source,
            publish: Arc::new(publish),
        }))
    }
    pub fn snapshot(&self) -> UpdateState {
        self.0.inner.lock().expect("update state").state.clone()
    }
    pub fn is_stopping(&self) -> bool {
        self.0.stopping.load(Ordering::SeqCst)
    }
    pub fn restart_ready(&self) -> bool {
        self.0.restart_ready.load(Ordering::SeqCst)
    }
    pub fn prepare_restart(&self) -> bool {
        if self
            .0
            .stopping
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return false;
        }
        self.0.restart_ready.store(true, Ordering::SeqCst);
        true
    }
    pub fn request_stop(&self) {
        self.0.stopping.store(true, Ordering::SeqCst);
        let inner = self.0.inner.lock().expect("update state");
        // A filesystem installation must finish; it is never aborted mid-swap.
        if inner.state.status != Status::Installing {
            if let Some(job) = &inner.job {
                let _ = job.cancel.try_send(());
            }
        }
    }
    async fn reap(&self) {
        let job = self.0.inner.lock().expect("update state").job.take();
        if let Some(job) = job {
            let _ = job.handle.await;
        }
    }
    fn publish(&self, state: UpdateState) {
        (self.0.publish)(state);
    }
    fn begin(inner: &mut Inner, status: Status) -> u64 {
        inner.active = true;
        inner.generation += 1;
        inner.state.revision += 1;
        inner.state.status = status;
        inner.state.notice = None;
        inner.state.downloaded = 0;
        inner.state.total = None;
        inner.generation
    }
    pub async fn check(&self, manual: bool) -> Result<UpdateState, Notice> {
        let _transition = self.0.transition.lock().await;
        if self.is_stopping() {
            return Err(Notice::Stopping);
        }
        {
            let mut inner = self.0.inner.lock().expect("update state");
            if inner.active {
                if inner.state.status == Status::Checking {
                    inner.manual |= manual;
                }
                return Ok(inner.state.clone());
            }
            if !inner.state.supported {
                return Ok(inner.state.clone());
            }
        }
        self.reap().await;
        if self.is_stopping() {
            return Err(Notice::Stopping);
        }
        let state = {
            let mut inner = self.0.inner.lock().expect("update state");
            let generation = Self::begin(&mut inner, Status::Checking);
            inner.manual = manual;
            inner.candidate = None;
            inner.bytes = None;
            inner.state.version = None;
            let (cancel, mut receiver) = mpsc::channel(1);
            let service = self.clone();
            let handle = tokio::spawn(async move {
                let result = tokio::select! { biased; _ = receiver.recv() => Err(Notice::Stopping), result = service.0.source.check() => result };
                let state = {
                    let mut inner = service.0.inner.lock().expect("update state");
                    if inner.generation != generation {
                        return;
                    }
                    inner.active = false;
                    match result {
                        Ok(Some(candidate)) => {
                            inner.state.version = Some(candidate.version().to_owned());
                            inner.candidate = Some(candidate);
                            inner.state.status = Status::Available;
                        }
                        Ok(None) => inner.state.status = Status::UpToDate,
                        Err(notice) => {
                            inner.state.status = Status::Idle;
                            // A manual click joining startup's in-flight check receives its error.
                            inner.state.notice =
                                (inner.manual && notice != Notice::Stopping).then_some(notice);
                        }
                    }
                    inner.state.revision += 1;
                    inner.state.clone()
                };
                service.publish(state);
            });
            inner.job = Some(Job { cancel, handle });
            inner.state.clone()
        };
        self.publish(state.clone());
        Ok(state)
    }
    pub async fn download(&self) -> Result<UpdateState, Notice> {
        let _transition = self.0.transition.lock().await;
        if self.is_stopping() {
            return Err(Notice::Stopping);
        }
        {
            let inner = self.0.inner.lock().expect("update state");
            if inner.state.installation != Installation::Managed {
                return Err(Notice::ManualInstall);
            }
            if inner.active || inner.state.status == Status::Ready {
                return Ok(inner.state.clone());
            }
            if inner.candidate.is_none() {
                return Err(Notice::NoUpdate);
            }
        }
        self.reap().await;
        if self.is_stopping() {
            return Err(Notice::Stopping);
        }
        let state = {
            let mut inner = self.0.inner.lock().expect("update state");
            let candidate = inner.candidate.clone().ok_or(Notice::NoUpdate)?;
            let generation = Self::begin(&mut inner, Status::Downloading);
            let (cancel, mut receiver) = mpsc::channel(1);
            let service = self.clone();
            let handle = tokio::spawn(async move {
                let progress_service = service.clone();
                let progress: Progress = Arc::new(move |chunk, total| {
                    progress_service.progress(generation, chunk, total)
                });
                let result = tokio::select! { biased; _ = receiver.recv() => Err(Notice::DownloadCancelled), result = candidate.download(progress) => result };
                let state = {
                    let mut inner = service.0.inner.lock().expect("update state");
                    if inner.generation != generation {
                        return;
                    }
                    inner.active = false;
                    inner.state.revision += 1;
                    match result {
                        Ok(bytes) => {
                            inner.bytes = Some(bytes);
                            inner.state.status = Status::Ready;
                        }
                        Err(notice) => {
                            inner.bytes = None;
                            inner.state.status = Status::Available;
                            inner.state.notice = Some(notice);
                        }
                    }
                    inner.state.clone()
                };
                service.publish(state);
            });
            inner.job = Some(Job { cancel, handle });
            inner.state.clone()
        };
        self.publish(state.clone());
        Ok(state)
    }
    fn progress(&self, generation: u64, chunk: usize, total: Option<u64>) {
        const MAX_SAFE: u64 = 9_007_199_254_740_991;
        let state = {
            let mut inner = self.0.inner.lock().expect("update state");
            if inner.generation != generation
                || !inner.active
                || inner.state.status != Status::Downloading
            {
                return;
            }
            let previous = inner.state.downloaded;
            let percent =
                |bytes: u64, length: Option<u64>| length.map(|n| bytes.saturating_mul(100) / n);
            let before = percent(previous, inner.state.total);
            inner.state.downloaded = previous.saturating_add(chunk as u64).min(MAX_SAFE);
            inner.state.total = total.filter(|n| *n > 0 && *n <= MAX_SAFE);
            if before == percent(inner.state.downloaded, inner.state.total)
                && inner.state.downloaded - previous < 1_048_576
            {
                return;
            }
            inner.state.revision += 1;
            inner.state.clone()
        };
        self.publish(state);
    }
    pub async fn cancel_download(&self) -> UpdateState {
        let _transition = self.0.transition.lock().await;
        {
            let inner = self.0.inner.lock().expect("update state");
            if inner.state.status != Status::Downloading {
                return inner.state.clone();
            }
            if let Some(job) = &inner.job {
                let _ = job.cancel.try_send(());
            }
        }
        self.reap().await;
        self.snapshot()
    }
    pub async fn install<F, Fut>(
        &self,
        confirmed: bool,
        coordinator: Coordinator,
        on_success: F,
    ) -> Result<UpdateState, Notice>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        if !confirmed {
            return Err(Notice::ConfirmationRequired);
        }
        let _transition = self.0.transition.lock().await;
        if self.is_stopping() {
            return Err(Notice::Stopping);
        }
        {
            let inner = self.0.inner.lock().expect("update state");
            if inner.state.installation != Installation::Managed {
                return Err(Notice::ManualInstall);
            }
            if inner.active {
                return Ok(inner.state.clone());
            }
            if inner.state.status != Status::Ready {
                return Err(Notice::NoUpdate);
            }
        }
        self.reap().await;
        if self.is_stopping() {
            return Err(Notice::Stopping);
        }
        let state = {
            let mut inner = self.0.inner.lock().expect("update state");
            let candidate = inner.candidate.clone().ok_or(Notice::NoUpdate)?;
            let bytes = inner.bytes.take().ok_or(Notice::NoUpdate)?;
            let generation = Self::begin(&mut inner, Status::Installing);
            let (cancel, _receiver) = mpsc::channel(1);
            let service = self.clone();
            let handle = tokio::spawn(async move {
                let suspension = coordinator.suspend_for_update().await;
                let result = match &suspension {
                    Ok(_) if !service.is_stopping() => candidate.install(bytes.clone()).await,
                    _ => Err(Notice::Stopping),
                };
                let successful = result.is_ok();
                let state = {
                    let mut inner = service.0.inner.lock().expect("update state");
                    if inner.generation != generation {
                        return;
                    }
                    inner.active = false;
                    inner.state.revision += 1;
                    if let Err(notice) = result {
                        inner.state.status = Status::Ready;
                        inner.state.notice = Some(notice);
                        inner.bytes = Some(bytes);
                        if notice == Notice::RestoreFailed {
                            inner.state.installation = Installation::Manual;
                            inner.state.status = Status::Available;
                            inner.bytes = None;
                        }
                    }
                    inner.state.clone()
                };
                if !successful {
                    drop(suspension);
                    service.publish(state);
                    return;
                }
                service.publish(state);
                // Keep the suspension through cleanup/restart; failure resumes reads before publishing.
                if !service.is_stopping() {
                    on_success().await;
                }
                drop(suspension);
            });
            inner.job = Some(Job { cancel, handle });
            inner.state.clone()
        };
        self.publish(state.clone());
        Ok(state)
    }
    pub async fn shutdown(&self) {
        self.request_stop();
        let _transition = self.0.transition.lock().await;
        self.reap().await;
    }
}

struct NativeSource(AppHandle);
impl Source for NativeSource {
    fn check(&self) -> Request<Option<Artifact>> {
        let app = self.0.clone();
        Box::pin(async move {
            let raw = app
                .config()
                .plugins
                .0
                .get("updater")
                .ok_or(Notice::NotConfigured)?;
            if raw
                .get("requireSignedVersion")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
                || raw
                    .get("pubkey")
                    .and_then(serde_json::Value::as_str)
                    .is_none_or(|key| key.trim().is_empty())
            {
                return Err(Notice::NotConfigured);
            }
            let updater = app
                .updater_builder()
                .timeout(Duration::from_secs(120))
                .version_comparator(|current, release| {
                    release.version.pre.is_empty() && release.version > current
                })
                // Default Windows cleanup runs before ShellExecute can fail. Our caller has
                // already drained CLI children; leave the app intact on installer launch failure.
                .on_before_exit(|| {})
                .build()
                .map_err(|_| Notice::NotConfigured)?;
            let found = tokio::time::timeout(Duration::from_secs(20), updater.check())
                .await
                .map_err(|_| Notice::CheckFailed)?
                .map_err(|_| Notice::CheckFailed)?;
            match found {
                Some(update) if update.download_url.scheme() == "https" => {
                    Ok(Some(Arc::new(NativeArtifact(update)) as Artifact))
                }
                Some(_) => Err(Notice::CheckFailed),
                None => Ok(None),
            }
        })
    }
}
struct NativeArtifact(tauri_plugin_updater::Update);
impl UpdateArtifact for NativeArtifact {
    fn version(&self) -> &str {
        &self.0.version
    }
    fn download(&self, progress: Progress) -> Request<Vec<u8>> {
        let update = self.0.clone();
        Box::pin(async move {
            // download verifies the signature AND announced version before bytes become installable.
            update
                .download(move |chunk, total| progress(chunk, total), || {})
                .await
                .map_err(|error| match error {
                    tauri_plugin_updater::Error::Minisign(_)
                    | tauri_plugin_updater::Error::Base64(_)
                    | tauri_plugin_updater::Error::SignatureUtf8(_)
                    | tauri_plugin_updater::Error::SignedVersionMismatch { .. }
                    | tauri_plugin_updater::Error::MissingSignedVersion => {
                        Notice::VerificationFailed
                    }
                    _ => Notice::DownloadFailed,
                })
        })
    }
    fn install(&self, bytes: Vec<u8>) -> Request<()> {
        let update = self.0.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                update.install(bytes).map_err(|error| match error {
                    tauri_plugin_updater::Error::PreviousAppNotRestored(_) => Notice::RestoreFailed,
                    _ => Notice::InstallFailed,
                })
            })
            .await
            .map_err(|_| Notice::InstallFailed)?
        })
    }
}
fn supported_platform() -> bool {
    cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(windows, target_arch = "x86_64")
    ))
}
fn installation(app: &AppHandle) -> Installation {
    if cfg!(debug_assertions) {
        return Installation::Development;
    }
    let Ok(executable) = std::env::current_exe() else {
        return Installation::Manual;
    };
    #[cfg(target_os = "macos")]
    if executable
        .parent()
        .is_some_and(|p| p.ends_with("Contents/MacOS"))
        && executable
            .ancestors()
            .nth(3)
            .is_some_and(|p| p.extension().is_some_and(|e| e == "app"))
    {
        return Installation::Managed;
    }
    #[cfg(windows)]
    {
        let expected = app.path().local_data_dir().ok().map(|p| {
            p.join(
                app.config()
                    .product_name
                    .as_deref()
                    .unwrap_or("KnowYourLimits"),
            )
        });
        if let (Some(parent), Some(expected)) = (executable.parent(), expected) {
            let parent = dunce::canonicalize(parent);
            let expected = dunce::canonicalize(expected);
            if let (Ok(parent), Ok(expected)) = (parent, expected) {
                if parent
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&expected.to_string_lossy())
                    && parent.join("uninstall.exe").is_file()
                {
                    return Installation::Managed;
                }
            }
        }
    }
    let _ = (app, executable);
    Installation::Manual
}

#[cfg(test)]
mod tests;
