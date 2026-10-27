//! Quota integration and persistence, shared by native commands and the CLI smoke.
use crate::alerts::{Alert, AlertTracker};
use crate::model::{now_ms, AppState, ProviderId, ProviderSnapshot, ProviderStatus, Settings};
use std::time::Instant;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::async_runtime::{
    channel, Mutex as AsyncMutex, Receiver, Sender, TokioHandle, TokioJoinHandle,
};

// Like Claude Desktop: a recent answer is reused for 60 s and a failure for 20 s,
// and the adaptive timer polls every 5 min after recent panel use, else every 15.
const REUSE_SUCCESS: std::time::Duration = std::time::Duration::from_secs(60);
const REUSE_FAILURE: std::time::Duration = std::time::Duration::from_secs(20);
const AUTO_ACTIVE: std::time::Duration = std::time::Duration::from_secs(30 * 60);

const CLAUDE_LOGIN_REQUIRED: &str = "kyl:loginClaude";
const STOPPING: &str = "kyl:stopping";
fn missing_cli(id: ProviderId) -> ReadFailure {
    ReadFailure::Message(match id {
        ProviderId::Claude => "kyl:installClaude",
        ProviderId::Codex => "kyl:installCodex",
    })
}
fn read_failed() -> ReadFailure {
    ReadFailure::Message("kyl:refreshFailed")
}
#[derive(Debug, Clone, Copy)]
enum ReadFailure {
    Message(&'static str),
    RateLimited { retry_after_secs: u32 },
}
impl ReadFailure {
    fn message(self) -> &'static str {
        match self {
            Self::Message(text) => text,
            Self::RateLimited { .. } => "kyl:rateLimited",
        }
    }
}
impl From<&'static str> for ReadFailure {
    fn from(value: &'static str) -> Self {
        Self::Message(value)
    }
}
type ReadFuture = Pin<Box<dyn Future<Output = Result<ProviderSnapshot, ReadFailure>> + Send>>;
type Reader = Arc<dyn Fn(ProviderId, bool, Receiver<()>) -> ReadFuture + Send + Sync>;
type Publisher = Arc<dyn Fn(AppState) + Send + Sync>;

#[derive(Clone)]
pub struct Coordinator(Arc<Core>);
struct Core {
    inner: Mutex<Inner>,
    reader: Reader,
    publish: Publisher,
    notify: Arc<dyn Fn(Alert) + Send + Sync>,
    // All launches/cancellations share this gate. The owner taking a join
    // handle holds it through cleanup, so shutdown can never miss that job.
    transition: AsyncMutex<()>,
    stopping: AtomicBool,
    update_suspended: AtomicBool,
    retry_path: Option<std::path::PathBuf>,
    quota_path: Option<std::path::PathBuf>,
    reuse: [std::time::Duration; 2],
}
struct Inner {
    state: AppState,
    generation: u64,
    job: Option<Job>,
    waiters: Vec<Sender<()>>,
    last_attempt: [Option<Instant>; 2],
    alerts: AlertTracker,
    blocked_until: [Option<Instant>; 2],
    fresh_until: [Option<Instant>; 2],
    last_activity: Option<Instant>,
}
struct Job {
    cancel: Sender<()>,
    handle: TokioJoinHandle<()>,
}
impl Coordinator {
    /// Construction never discovers or executes either CLI. Publication is synchronous
    /// and outside the snapshot lock; native consumers must re-read for tray UI.
    pub fn new(state: AppState, publish: impl Fn(AppState) + Send + Sync + 'static) -> Self {
        Self::with_reader(
            state,
            Arc::new(|id, sign_in, cancel| Box::pin(production_read(id, sign_in, cancel))),
            Arc::new(publish),
        )
        .with_reuse(REUSE_SUCCESS, REUSE_FAILURE)
    }
    fn with_reader(state: AppState, reader: Reader, publish: Publisher) -> Self {
        Self(Arc::new(Core {
            inner: Mutex::new(Inner {
                state,
                generation: 0,
                job: None,
                waiters: vec![],
                last_attempt: [None; 2],
                alerts: AlertTracker::default(),
                blocked_until: [None; 2],
                fresh_until: [None; 2],
                last_activity: None,
            }),
            reader,
            publish,
            notify: Arc::new(|_| {}),
            transition: AsyncMutex::new(()),
            stopping: AtomicBool::new(false),
            update_suspended: AtomicBool::new(false),
            retry_path: None,
            quota_path: None,
            reuse: [std::time::Duration::ZERO; 2],
        }))
    }
    fn with_reuse(mut self, success: std::time::Duration, failure: std::time::Duration) -> Self {
        Arc::get_mut(&mut self.0).expect("new coordinator").reuse = [success, failure];
        self
    }
    /// The panel was shown: the adaptive timer switches to its short interval.
    pub fn note_activity(&self) {
        self.0
            .inner
            .lock()
            .expect("coordinator state")
            .last_activity = Some(Instant::now());
    }
    pub fn new_with_notifier(
        state: AppState,
        publish: impl Fn(AppState) + Send + Sync + 'static,
        notify: impl Fn(Alert) + Send + Sync + 'static,
    ) -> Self {
        let mut coordinator = Self::new(state, publish);
        Arc::get_mut(&mut coordinator.0)
            .expect("new coordinator")
            .notify = Arc::new(notify);
        coordinator
    }
    pub async fn refresh_due(&self) -> Result<AppState, String> {
        let id = {
            let inner = self.0.inner.lock().expect("coordinator state");
            due_provider(&inner, Instant::now())
        };
        match id {
            Some(id) => self.start(id, false, true).await,
            None => Ok(self.snapshot()),
        }
    }
    pub fn with_persistence(mut self, directory: std::path::PathBuf) -> Self {
        let core = Arc::get_mut(&mut self.0).expect("new coordinator");
        let now = now_ms();
        let retry_path = directory.join("quota-retry.json");
        let quota_path = directory.join("quota-cache.json");
        let retry = crate::storage::load_retry(&retry_path, now);
        {
            let mut inner = core.inner.lock().expect("coordinator state");
            if let Err(error) = crate::storage::restore_quota(&quota_path, &mut inner.state, now) {
                inner.state.notice = Some(
                    [inner.state.notice.as_deref(), Some(&error)]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
            for id in [ProviderId::Claude, ProviderId::Codex] {
                // Existing 0.2.1 cooldown is evidence of an opted-in provider,
                // including when this is the first version with a quota cache.
                if retry.get(id).is_some() && !inner.state.connected.contains(&id) {
                    inner.state.connected.push(id);
                }
                if inner.state.connected.contains(&id) {
                    inner.last_attempt[id.index()] = Some(Instant::now());
                }
                inner.blocked_until[id.index()] = retry.get(id).map(|until| {
                    Instant::now() + std::time::Duration::from_millis(until.saturating_sub(now))
                });
            }
            inner.state.retry_at = retry;
        }
        core.retry_path = Some(retry_path);
        core.quota_path = Some(quota_path);
        self
    }
    pub fn snapshot(&self) -> AppState {
        self.0
            .inner
            .lock()
            .expect("coordinator state")
            .state
            .clone()
    }
    pub fn is_stopping(&self) -> bool {
        self.0.stopping.load(Ordering::SeqCst)
    }
    pub fn request_stop(&self) {
        self.0.stopping.store(true, Ordering::SeqCst);
        if let Some(job) = &self.0.inner.lock().expect("coordinator state").job {
            let _ = job.cancel.try_send(());
        }
    }
    pub async fn read_quota(&self, provider: ProviderId) -> Result<AppState, String> {
        self.start(provider, false, false).await
    }
    pub async fn sign_in_claude(&self) -> Result<AppState, String> {
        self.start(ProviderId::Claude, true, false).await
    }
    async fn start(
        &self,
        provider: ProviderId,
        sign_in: bool,
        automatic: bool,
    ) -> Result<AppState, String> {
        let transition = self.0.transition.lock().await;
        {
            let inner = self.0.inner.lock().expect("coordinator state");
            if self.is_stopping() || self.0.update_suspended.load(Ordering::SeqCst) {
                return Err(STOPPING.into());
            }
            let now = Instant::now();
            let waiting = |until: Option<Instant>| until.is_some_and(|until| until > now);
            let index = provider.index();
            if !sign_in
                && (waiting(inner.blocked_until[index]) || waiting(inner.fresh_until[index]))
            {
                return Ok(inner.state.clone());
            }
            if automatic && due_provider(&inner, Instant::now()) != Some(provider) {
                return Ok(inner.state.clone());
            }
            if let Some(current) = inner.state.refreshing {
                return if current == provider && sign_in == inner.state.signing_in {
                    Ok(inner.state.clone())
                } else {
                    Err("kyl:waitRefresh".into())
                };
            }
        }
        // Reap the preceding completed job before replacing its owned handle.
        self.cancel_and_join().await;
        let state = {
            let mut inner = self.0.inner.lock().expect("coordinator state");
            if self.is_stopping() || self.0.update_suspended.load(Ordering::SeqCst) {
                return Err(STOPPING.into());
            }
            if !inner.state.connected.contains(&provider) {
                inner.state.connected.push(provider);
            }
            let target = inner
                .state
                .providers
                .iter_mut()
                .find(|p| p.id == provider)
                .expect("provider");
            target.status = if target.fetched_at.is_some() {
                ProviderStatus::Stale
            } else {
                ProviderStatus::Unavailable
            };
            inner.last_attempt[provider.index()] = Some(Instant::now());
            inner.state.refreshing = Some(provider);
            inner.state.signing_in = sign_in;
            inner.state.notice = None;
            inner.state.revision += 1;
            let generation = inner.generation;
            let (cancel, receiver) = channel(1);
            let coordinator = self.clone();
            let handle = TokioHandle::current().spawn(async move {
                let result = if coordinator.is_stopping() {
                    Err(STOPPING.into())
                } else {
                    (coordinator.0.reader)(provider, sign_in, receiver).await
                };
                coordinator.complete(generation, provider, result);
            });
            inner.job = Some(Job { cancel, handle });
            inner.state.clone()
        };
        drop(transition);
        (self.0.publish)(state.clone());
        Ok(state)
    }
    fn complete(
        &self,
        generation: u64,
        provider: ProviderId,
        result: Result<ProviderSnapshot, ReadFailure>,
    ) {
        let (state, alert) = {
            let mut inner = self.0.inner.lock().expect("coordinator state");
            if self.is_stopping()
                || inner.generation != generation
                || inner.state.refreshing != Some(provider)
            {
                return;
            }
            let failure = result.as_ref().err().copied();
            let reuse = match &result {
                Ok(snapshot) if snapshot.id == provider && !snapshot.windows.is_empty() => {
                    self.0.reuse[0]
                }
                // The rate-limit cooldown below already defers the next read.
                Err(ReadFailure::RateLimited { .. }) => std::time::Duration::ZERO,
                _ => self.0.reuse[1],
            };
            inner.fresh_until[provider.index()] =
                (!reuse.is_zero()).then(|| Instant::now() + reuse);
            if let Some(ReadFailure::RateLimited { retry_after_secs }) = failure {
                let delay = std::time::Duration::from_secs(u64::from(retry_after_secs));
                inner.blocked_until[provider.index()] = Some(Instant::now() + delay);
                inner.state.retry_at.set(
                    provider,
                    Some(now_ms() + u64::from(retry_after_secs) * 1000),
                );
            } else {
                inner.blocked_until[provider.index()] = None;
                inner.state.retry_at.set(provider, None);
            }
            let target = inner
                .state
                .providers
                .iter_mut()
                .find(|p| p.id == provider)
                .expect("provider");
            let notice = match result {
                Ok(snapshot) if snapshot.id == provider => {
                    let notice = snapshot
                        .remaining(now_ms())
                        .is_none()
                        .then(|| "kyl:noQuota".into());
                    if snapshot.windows.is_empty() && target.fetched_at.is_some() {
                        target.status = ProviderStatus::Stale;
                    } else {
                        *target = snapshot;
                    }
                    notice
                }
                error => {
                    let notice = error.err().unwrap_or_else(read_failed).message();
                    target.status = if notice == CLAUDE_LOGIN_REQUIRED {
                        ProviderStatus::SignedOut
                    } else {
                        ProviderStatus::Error
                    };
                    Some(notice.into())
                }
            };
            inner.last_attempt[provider.index()] = Some(Instant::now());
            inner.state.notice = notice;
            for failure in [
                self.0
                    .retry_path
                    .as_ref()
                    .and_then(|path| crate::storage::save_retry(path, &inner.state.retry_at).err()),
                self.0
                    .quota_path
                    .as_ref()
                    .and_then(|path| crate::storage::save_quota(path, &inner.state).err()),
            ]
            .into_iter()
            .flatten()
            {
                inner.state.notice = Some(match inner.state.notice.take() {
                    Some(notice) => format!("{notice} {failure}"),
                    None => failure,
                });
            }
            inner.state.refreshing = None;
            inner.state.signing_in = false;
            inner.state.revision += 1;
            let snapshot = inner
                .state
                .providers
                .iter()
                .find(|p| p.id == provider)
                .expect("provider")
                .clone();
            let settings = inner.state.settings.clone();
            let alert = inner.alerts.observe(&snapshot, &settings, now_ms());
            wake_waiters(&mut inner);
            (inner.state.clone(), alert)
        };
        (self.0.publish)(state);
        if let Some(alert) = alert {
            (self.0.notify)(alert);
        }
    }
    /// Wait for the currently committed read to settle, without taking its owned
    /// join handle or preventing cancellation by a simultaneous cancellation/exit action.
    pub async fn wait_for_idle(&self) -> AppState {
        loop {
            let receiver = {
                let mut inner = self.0.inner.lock().expect("coordinator state");
                if inner.state.refreshing.is_none() {
                    return inner.state.clone();
                }
                let (sender, receiver) = channel(1);
                inner.waiters.retain(|waiter| !waiter.is_closed());
                inner.waiters.push(sender);
                receiver
            };
            let mut receiver = receiver;
            let _ = receiver.recv().await;
        }
    }
    pub fn save_settings(
        &self,
        settings: Settings,
        settings_path: &std::path::Path,
    ) -> Result<AppState, String> {
        self.update_settings(settings_path, |_| settings.validated())
    }
    /// Apply only this provider's timer, without committing a UI preferences draft.
    pub fn set_refresh_minutes(
        &self,
        provider: ProviderId,
        minutes: u16,
        settings_path: &std::path::Path,
    ) -> Result<AppState, String> {
        self.update_settings(settings_path, |mut settings| {
            match provider {
                ProviderId::Claude => settings.preferences.claude.refresh_minutes = minutes,
                ProviderId::Codex => settings.preferences.codex.refresh_minutes = minutes,
            }
            settings.validated()
        })
    }
    fn update_settings(
        &self,
        settings_path: &std::path::Path,
        change: impl FnOnce(Settings) -> Result<Settings, String>,
    ) -> Result<AppState, String> {
        let state = {
            let mut inner = self.0.inner.lock().expect("coordinator state");
            if self.is_stopping() {
                return Err(STOPPING.into());
            }
            let settings = change(inner.state.settings.clone())?;
            crate::storage::save(settings_path, &settings)?;
            for id in [ProviderId::Claude, ProviderId::Codex] {
                if inner.state.settings.preferences.get(id).refresh_minutes
                    != settings.preferences.get(id).refresh_minutes
                {
                    inner.last_attempt[id.index()] = Some(Instant::now());
                }
                if inner
                    .state
                    .settings
                    .preferences
                    .get(id)
                    .notifications
                    .enabled
                    != settings.preferences.get(id).notifications.enabled
                    || inner.state.settings.thresholds.get(id) != settings.thresholds.get(id)
                {
                    inner.alerts.reset(id);
                }
            }
            inner.state.settings = settings;
            inner.state.revision += 1;
            inner.state.clone()
        };
        (self.0.publish)(state.clone());
        Ok(state)
    }
    pub async fn cancel_read(&self) -> Result<AppState, String> {
        let transition = self.0.transition.lock().await;
        if self.is_stopping() {
            return Err(STOPPING.into());
        }
        self.cancel_and_join().await;
        let state = {
            let mut inner = self.0.inner.lock().expect("coordinator state");
            if self.is_stopping() {
                return Err(STOPPING.into());
            }
            if inner.state.refreshing.take().is_some() {
                inner.state.revision += 1;
                inner.state.notice = Some("kyl:cancelled".into());
            }
            inner.state.signing_in = false;
            wake_waiters(&mut inner);
            inner.state.clone()
        };
        drop(transition);
        (self.0.publish)(state.clone());
        Ok(state)
    }
    // Only called while holding the asynchronous transition gate, never a sync lock.
    async fn cancel_and_join(&self) {
        let job = {
            let mut inner = self.0.inner.lock().expect("coordinator state");
            inner.generation += 1;
            inner.job.take()
        };
        if let Some(job) = job {
            let _ = job.cancel.try_send(());
            let _ = job.handle.await;
        }
    }
    pub async fn shutdown(&self) {
        self.request_stop();
        let _transition = self.0.transition.lock().await;
        self.cancel_and_join().await;
        let state = {
            let mut inner = self.0.inner.lock().expect("coordinator state");
            if inner.state.refreshing.take().is_some() {
                inner.state.signing_in = false;
                inner.state.revision += 1;
            }
            wake_waiters(&mut inner);
            inner.state.clone()
        };
        drop(_transition);
        (self.0.publish)(state);
    }

    /// Block new CLI launches and await the existing owned reader's cleanup.
    /// The guard resumes readings if installation fails; real shutdown stays final.
    pub(crate) async fn suspend_for_update(&self) -> Result<UpdateSuspension, String> {
        let _transition = self.0.transition.lock().await;
        if self.is_stopping() || self.0.update_suspended.swap(true, Ordering::SeqCst) {
            return Err(STOPPING.into());
        }
        let suspension = UpdateSuspension(self.clone());
        self.cancel_and_join().await;
        let state = {
            let mut inner = self.0.inner.lock().expect("coordinator state");
            inner.state.refreshing = None;
            inner.state.signing_in = false;
            inner.state.revision += 1;
            wake_waiters(&mut inner);
            inner.state.clone()
        };
        (self.0.publish)(state);
        Ok(suspension)
    }
}

pub(crate) struct UpdateSuspension(Coordinator);
impl Drop for UpdateSuspension {
    fn drop(&mut self) {
        self.0 .0.update_suspended.store(false, Ordering::SeqCst);
    }
}
fn due_provider(inner: &Inner, now: Instant) -> Option<ProviderId> {
    if inner.state.refreshing.is_some() {
        return None;
    }
    inner
        .state
        .connected
        .iter()
        .copied()
        .filter_map(|id| {
            if inner.blocked_until[id.index()].is_some_and(|until| until > now) {
                return None;
            }
            let minutes = match inner.state.settings.preferences.get(id).refresh_minutes {
                crate::model::AUTO_REFRESH_MINUTES => {
                    let active = inner
                        .last_activity
                        .is_some_and(|at| now.saturating_duration_since(at) < AUTO_ACTIVE);
                    if active {
                        5
                    } else {
                        15
                    }
                }
                minutes => minutes,
            };
            let last = inner.last_attempt[id.index()]?;
            let deadline = last + std::time::Duration::from_secs(u64::from(minutes) * 60);
            (minutes > 0 && now >= deadline).then_some((id, deadline))
        })
        .min_by_key(|(_, deadline)| *deadline)
        .map(|(id, _)| id)
}
fn wake_waiters(inner: &mut Inner) {
    for waiter in inner.waiters.drain(..) {
        let _ = waiter.try_send(());
    }
}
struct Resolved {
    executable: std::path::PathBuf,
    child_path: std::ffi::OsString,
}
fn user_home() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    let key = "USERPROFILE";
    #[cfg(not(windows))]
    let key = "HOME";
    std::env::var_os(key).map(Into::into)
}
#[cfg(unix)]
fn known_dirs(home: Option<std::path::PathBuf>) -> Vec<std::path::PathBuf> {
    // Prefer managed installs over user-bin shadows; never execute probes to pick one.
    let mut dirs = vec!["/opt/homebrew/bin".into(), "/usr/local/bin".into()];
    if let Some(home) = home.filter(|home| home.is_absolute()) {
        for bin in [".bun/bin", ".local/bin", ".npm-global/bin", ".volta/bin"] {
            dirs.push(home.join(bin));
        }
    }
    dirs.push("/usr/bin".into());
    dirs
}
#[cfg(unix)]
fn resolve_in(id: ProviderId, dirs: &[std::path::PathBuf]) -> Result<Resolved, ReadFailure> {
    use std::os::unix::fs::PermissionsExt;
    // Reject synthetic or malformed relative locations rather than consulting cwd.
    if dirs.iter().any(|dir| !dir.is_absolute()) {
        return Err(missing_cli(id));
    }
    let executable = dirs
        .iter()
        .map(|dir| dir.join(id.label().to_lowercase()))
        .find(|path| {
            std::fs::metadata(path)
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
        .ok_or(missing_cli(id))?;
    let child_path = std::env::join_paths(
        dirs.iter()
            .map(|p| p.as_path())
            .chain(["/bin", "/usr/sbin", "/sbin"].map(std::path::Path::new)),
    )
    .map_err(|_| missing_cli(id))?;
    Ok(Resolved {
        executable,
        child_path,
    })
}
#[cfg(windows)]
fn known_dirs(home: Option<std::path::PathBuf>) -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = home.filter(|p| p.is_absolute()) {
        for bin in [".local/bin", ".bun/bin", ".cargo/bin"] {
            dirs.push(home.join(bin));
        }
    }
    for (key, suffix) in [
        ("APPDATA", "npm"),
        ("LOCALAPPDATA", "Microsoft/WinGet/Links"),
        ("ProgramFiles", "nodejs"),
    ] {
        if let Some(base) = std::env::var_os(key)
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute())
        {
            dirs.push(base.join(suffix));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path).filter(|p| p.is_absolute()) {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    dirs
}
#[cfg(windows)]
fn resolve_in(id: ProviderId, dirs: &[std::path::PathBuf]) -> Result<Resolved, ReadFailure> {
    if dirs.iter().any(|p| !p.is_absolute()) {
        return Err(missing_cli(id));
    }
    let name = id.label().to_lowercase();
    let executable = dirs
        .iter()
        .flat_map(|dir| {
            [
                dir.join(format!("{name}.exe")),
                dir.join(format!("{name}.cmd")),
            ]
        })
        .find(|p| {
            if !p.is_file() {
                return false;
            }
            if p.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
            {
                return true;
            }
            let entry = match id {
                ProviderId::Codex => "node_modules/@openai/codex/bin/codex.js",
                ProviderId::Claude => "node_modules/@anthropic-ai/claude-code/cli.js",
            };
            p.parent().is_some_and(|dir| dir.join(entry).is_file())
        })
        .ok_or(missing_cli(id))?;
    let child_path = std::env::join_paths(dirs).map_err(|_| missing_cli(id))?;
    Ok(Resolved {
        executable,
        child_path,
    })
}

async fn read_resolved(
    id: ProviderId,
    sign_in: bool,
    resolved: Result<Resolved, ReadFailure>,
    cancel: Receiver<()>,
) -> Result<ProviderSnapshot, ReadFailure> {
    let resolved = resolved?;
    match id {
        ProviderId::Codex => {
            crate::codex::read_managed(&resolved.executable, &resolved.child_path, cancel)
                .await
                .map(|r| r.snapshot)
                .map_err(|_| read_failed())
        }
        ProviderId::Claude => crate::claude::read_or_sign_in(
            &resolved.executable,
            &resolved.child_path,
            cancel,
            sign_in,
        )
        .await
        .map_err(|e| match e {
            crate::claude::ClaudeError::NotAuthenticated => CLAUDE_LOGIN_REQUIRED.into(),
            crate::claude::ClaudeError::RateLimited { retry_after_secs } => {
                ReadFailure::RateLimited { retry_after_secs }
            }
            crate::claude::ClaudeError::UnsupportedAccount => "kyl:signInHelp".into(),
            crate::claude::ClaudeError::UnsupportedVersion => "kyl:updateClaude".into(),
            crate::claude::ClaudeError::Timeout if sign_in => "kyl:signInTimeout".into(),
            _ => read_failed(),
        }),
    }
}
async fn production_read(
    id: ProviderId,
    sign_in: bool,
    cancel: Receiver<()>,
) -> Result<ProviderSnapshot, ReadFailure> {
    #[cfg(any(unix, windows))]
    {
        read_resolved(
            id,
            sign_in,
            resolve_in(id, &known_dirs(user_home())),
            cancel,
        )
        .await
    }
    #[cfg(not(any(unix, windows)))]
    {
        drop(cancel);
        let _ = (id, sign_in);
        Err("kyl:appOnly".into())
    }
}

#[cfg(all(test, unix))]
mod tests;
