use crate::model::Settings;
use crate::model::{AppState, ProviderId, ProviderSnapshot, ProviderStatus};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

pub fn load(path: &Path) -> (Settings, Option<String>) {
    let contents = match fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (Settings::default(), None)
        }
        Err(_) => {
            return (
                Settings::default(),
                Some("kyl:defaultSettingsNotice".into()),
            )
        }
    };
    match serde_json::from_slice::<Settings>(&contents)
        .map_err(|e| e.to_string())
        .and_then(Settings::validated)
    {
        Ok(settings) => (settings, None),
        Err(_) => (
            Settings::default(),
            Some("kyl:defaultSettingsNotice".into()),
        ),
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaCache {
    version: u8,
    connected: Vec<ProviderId>,
    providers: Vec<ProviderSnapshot>,
}

/// Only normalized quota observations and opted-in providers enter this file.
/// Settings, credentials, raw CLI output and in-flight jobs are not serialized.
pub fn save_quota(path: &Path, state: &AppState) -> Result<(), String> {
    let cache = QuotaCache {
        version: 1,
        connected: state.connected.clone(),
        providers: state
            .providers
            .iter()
            .filter(|p| p.fetched_at.is_some() && !p.windows.is_empty())
            .cloned()
            .collect(),
    };
    let bytes = serde_json::to_vec(&cache).map_err(|_| "kyl:errorCacheSave")?;
    write_private(path, &bytes).map_err(|_| "kyl:errorCacheSave".into())
}

pub fn restore_quota(path: &Path, state: &mut AppState, now: u64) -> Result<(), String> {
    use std::io::Read;
    const MAX_BYTES: u64 = 65_536;
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("kyl:cacheFailed".into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "kyl:cacheFailed")?;
    let invalid = "kyl:cacheFailed";
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid.into());
    }
    let mut cache: QuotaCache = serde_json::from_slice(&bytes).map_err(|_| invalid)?;
    if cache.version != 1
        || cache.connected.len() > 2
        || cache.providers.len() > 2
        || cache
            .connected
            .iter()
            .enumerate()
            .any(|(i, id)| cache.connected[..i].contains(id))
        || cache.providers.iter().enumerate().any(|(i, p)| {
            !cache.connected.contains(&p.id)
                || cache.providers[..i].iter().any(|other| other.id == p.id)
                || p.fetched_at.is_none_or(|at| at > now)
                || p.windows.is_empty()
                || p.windows.len() > 16
                || p.windows.iter().enumerate().any(|(j, w)| {
                    w.id.is_empty()
                        || w.id.len() > 160
                        || w.label.is_empty()
                        || w.label.len() > 160
                        || !w.used_percent.is_finite()
                        || !(0.0..=100.0).contains(&w.used_percent)
                        || w.duration_mins == 0
                        || w.resets_at.is_some_and(|at| at > 9_007_199_254_740_991)
                        || p.windows[..j].iter().any(|other| other.id == w.id)
                })
        })
    {
        return Err(invalid.into());
    }
    state.connected = cache.connected;
    for p in &mut cache.providers {
        p.label = p.id.label().into();
        if p.status != ProviderStatus::SignedOut {
            p.status = ProviderStatus::Stale;
        }
        if let Some(target) = state.providers.iter_mut().find(|target| target.id == p.id) {
            *target = p.clone();
        }
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("missing cache directory"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    let settings = settings.clone().validated()?;
    let bytes = serde_json::to_vec_pretty(&settings).map_err(|_| "kyl:errorSave")?;
    let parent = path.parent().ok_or("kyl:errorSave")?;
    fs::create_dir_all(parent).map_err(|_| "kyl:errorSave")?;
    let temporary = path.with_extension("json.tmp");
    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err("kyl:errorSave".into());
    }
    Ok(())
}

/// App-owned retry metadata contains only provider deadlines, never CLI data.
pub fn load_retry(path: &Path, now: u64) -> crate::model::RetryAt {
    let mut retry = fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<crate::model::RetryAt>(&bytes).ok())
        .unwrap_or_default();
    for id in [
        crate::model::ProviderId::Claude,
        crate::model::ProviderId::Codex,
    ] {
        let until = retry
            .get(id)
            .filter(|until| *until > now)
            .map(|until| until.min(now.saturating_add(3_600_000)));
        retry.set(id, until);
    }
    retry
}
pub fn save_retry(path: &Path, retry: &crate::model::RetryAt) -> Result<(), String> {
    let parent = path.parent().ok_or("kyl:errorPauseSave")?;
    fs::create_dir_all(parent).map_err(|_| "kyl:errorPauseSave")?;
    let bytes = serde_json::to_vec(retry).map_err(|_| "kyl:errorPauseSave")?;
    let temporary = path.with_extension("json.tmp");
    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err("kyl:errorPauseSave".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{now_ms, ProviderId};
    use std::path::PathBuf;

    struct TestDir(PathBuf);
    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "kyl-test-{}-{}-{}",
                std::process::id(),
                now_ms(),
                name
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn settings(&self) -> PathBuf {
            self.0.join("settings.json")
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn round_trip_retains_selection_order_and_thresholds() {
        let dir = TestDir::new("roundtrip");
        let mut settings = Settings {
            bar_providers: vec![ProviderId::Codex, ProviderId::Claude],
            ..Settings::default()
        };
        settings.thresholds.claude = vec![75, 92, 100];
        save(&dir.settings(), &settings).unwrap();
        let (loaded, notice) = load(&dir.settings());
        assert_eq!(loaded, settings);
        assert!(notice.is_none());
        assert!(!dir.settings().with_extension("json.tmp").exists());
    }

    #[test]
    fn missing_file_has_defaults_without_an_error() {
        let dir = TestDir::new("missing");
        assert_eq!(load(&dir.settings()), (Settings::default(), None));
    }

    fn quota_state() -> AppState {
        let mut state = AppState::disconnected(Settings::default());
        state.connected = vec![ProviderId::Codex];
        state.providers[1] = ProviderSnapshot {
            id: ProviderId::Codex,
            label: "Codex".into(),
            status: ProviderStatus::Current,
            fetched_at: Some(1_800_000_000_000),
            windows: vec![crate::model::QuotaWindow {
                id: "weekly".into(),
                label: "Неделя".into(),
                used_percent: 90.0,
                resets_at: Some(1_800_001_000_000),
                duration_mins: 10080,
                primary: true,
            }],
        };
        state
    }

    #[test]
    fn cache_is_private_and_does_not_store_transient_jobs_settings_or_notices() {
        let dir = TestDir::new("quota-cache");
        let path = dir.0.join("quota-cache.json");
        let mut original = quota_state();
        original.refreshing = Some(ProviderId::Codex);
        original.signing_in = true;
        original.notice = Some("SYNTHETIC_PRIVATE".into());
        save_quota(&path, &original).unwrap();
        let bytes = fs::read(&path).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.get("settings").is_none());
        assert!(json.get("refreshing").is_none());
        assert!(!String::from_utf8(bytes)
            .unwrap()
            .contains("SYNTHETIC_PRIVATE"));
        let mut restored = AppState::disconnected(Settings::default());
        restore_quota(&path, &mut restored, 1_800_002_000_000).unwrap();
        assert_eq!(
            restored.providers[1].fetched_at,
            original.providers[1].fetched_at
        );
        assert_eq!(restored.title(1_800_002_000_000), "C — · X ~10%");
        assert!(restored.refreshing.is_none());
        assert!(!restored.signing_in);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn invalid_cache_never_replaces_state_or_changes_the_source_file() {
        let dir = TestDir::new("invalid-quota-cache");
        let path = dir.0.join("quota-cache.json");
        save_quota(&path, &quota_state()).unwrap();
        let valid: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let mut future = valid.clone();
        future["providers"][0]["fetchedAt"] = serde_json::json!(1_800_000_000_001u64);
        let mut bad_percent = valid.clone();
        bad_percent["providers"][0]["windows"][0]["usedPercent"] = serde_json::json!(101);
        let mut duplicate = valid.clone();
        duplicate["connected"] = serde_json::json!(["codex", "codex"]);
        let mut version = valid.clone();
        version["version"] = serde_json::json!(2);
        for bytes in [
            b"not-json".to_vec(),
            vec![b' '; 65_537],
            serde_json::to_vec(&future).unwrap(),
            serde_json::to_vec(&bad_percent).unwrap(),
            serde_json::to_vec(&duplicate).unwrap(),
            serde_json::to_vec(&version).unwrap(),
        ] {
            fs::write(&path, &bytes).unwrap();
            let mut state = AppState::disconnected(Settings::default());
            assert!(restore_quota(&path, &mut state, 1_800_000_000_000).is_err());
            assert!(state.connected.is_empty());
            assert!(state.providers[1].windows.is_empty());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn corrupt_file_is_reported_and_not_replaced_during_loading() {
        let dir = TestDir::new("corrupt");
        fs::write(dir.settings(), b"not-json").unwrap();
        let (settings, notice) = load(&dir.settings());
        assert_eq!(settings, Settings::default());
        assert!(notice.is_some());
        assert_eq!(fs::read(dir.settings()).unwrap(), b"not-json");
    }

    #[test]
    fn invalid_settings_do_not_replace_a_successful_save() {
        let dir = TestDir::new("invalid");
        save(&dir.settings(), &Settings::default()).unwrap();
        let before = fs::read(dir.settings()).unwrap();
        let mut invalid = Settings::default();
        invalid.bar_providers.clear();
        assert!(save(&dir.settings(), &invalid).is_err());
        assert_eq!(fs::read(dir.settings()).unwrap(), before);
    }

    #[test]
    fn failed_rename_is_reported_and_temporary_file_is_cleaned_up() {
        let dir = TestDir::new("failure");
        fs::create_dir(dir.settings()).unwrap();
        assert!(save(&dir.settings(), &Settings::default()).is_err());
        assert!(dir.settings().is_dir());
        assert!(!dir.settings().with_extension("json.tmp").exists());
    }
    #[test]
    fn legacy_settings_migrate_without_losing_choices_or_overwriting_the_file() {
        let dir = TestDir::new("legacy");
        let original =
            br#"{"barProviders":["codex"],"thresholds":{"claude":[70,90],"codex":[75,95]}}"#;
        fs::write(dir.settings(), original).unwrap();
        let (settings, notice) = load(&dir.settings());
        assert!(notice.is_none());
        assert_eq!(settings.bar_providers, vec![ProviderId::Codex]);
        assert_eq!(settings.thresholds.claude, vec![70, 90]);
        assert_eq!(settings.preferences.codex.refresh_minutes, 0);
        assert!(!settings.preferences.claude.notifications.enabled);
        assert_eq!(settings.layout, crate::model::TrayLayout::Stacked);
        assert_eq!(settings.language, "system");
        assert_eq!(fs::read(dir.settings()).unwrap(), original);
    }
    #[test]
    fn provider_preferences_round_trip_with_independent_timers_colors_and_alerts() {
        let dir = TestDir::new("preferences");
        let mut settings = Settings {
            language: "fi".into(),
            layout: crate::model::TrayLayout::Rings,
            ..Settings::default()
        };
        settings.preferences.claude.color = "#aa6633".into();
        settings.preferences.claude.refresh_minutes = 2;
        settings.preferences.claude.notifications.enabled = true;
        settings.preferences.codex.refresh_minutes = 15;
        settings.preferences.codex.size = crate::model::IndicatorSize::Small;
        save(&dir.settings(), &settings).unwrap();
        assert_eq!(load(&dir.settings()), (settings, None));
    }
}
