use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};

pub const STALE_AFTER_MS: u64 = 300_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Claude,
    Codex,
}

impl ProviderId {
    pub fn index(self) -> usize {
        match self {
            Self::Claude => 0,
            Self::Codex => 1,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Self::Claude => "C",
            Self::Codex => "X",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderStatus {
    Current,
    Blocked,
    Stale,
    Error,
    Unavailable,
    SignedOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thresholds {
    pub claude: Vec<u8>,
    pub codex: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "crate::i18n::default_language")]
    pub language: String,
    pub bar_providers: Vec<ProviderId>,
    pub thresholds: Thresholds,
    #[serde(default)]
    pub layout: TrayLayout,
    #[serde(default)]
    pub preferences: Preferences,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrayLayout {
    Inline,
    #[default]
    Stacked,
    Rings,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconStyle {
    #[default]
    Symbol,
    Letter,
    Dot,
    None,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndicatorSize {
    Small,
    #[default]
    Medium,
    Large,
}
impl IndicatorSize {
    pub fn scale(self) -> f64 {
        match self {
            Self::Small => 0.85,
            Self::Medium => 1.0,
            Self::Large => 1.15,
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettings {
    pub enabled: bool,
    pub sound: bool,
    pub recovery: bool,
}
/// `refresh_minutes` value for the adaptive timer: 5 min after recent panel use, else 15.
pub const AUTO_REFRESH_MINUTES: u16 = u16::MAX;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreferences {
    pub icon: IconStyle,
    pub color: String,
    pub size: IndicatorSize,
    pub refresh_minutes: u16,
    pub notifications: NotificationSettings,
}
impl Default for ProviderPreferences {
    fn default() -> Self {
        Self {
            icon: IconStyle::default(),
            color: "system".into(),
            size: IndicatorSize::default(),
            refresh_minutes: 0,
            notifications: NotificationSettings::default(),
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub claude: ProviderPreferences,
    pub codex: ProviderPreferences,
}
impl Preferences {
    pub fn get(&self, id: ProviderId) -> &ProviderPreferences {
        match id {
            ProviderId::Claude => &self.claude,
            ProviderId::Codex => &self.codex,
        }
    }
}
impl Thresholds {
    pub fn get(&self, id: ProviderId) -> &[u8] {
        match id {
            ProviderId::Claude => &self.claude,
            ProviderId::Codex => &self.codex,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: crate::i18n::default_language(),
            bar_providers: vec![ProviderId::Claude, ProviderId::Codex],
            thresholds: Thresholds {
                claude: vec![80, 95, 100],
                codex: vec![80, 95, 100],
            },
            layout: TrayLayout::default(),
            preferences: Preferences::default(),
        }
    }
}

impl Settings {
    pub fn validated(mut self) -> Result<Self, String> {
        if !crate::i18n::valid_language(&self.language) {
            return Err("kyl:invalidLanguage".into());
        }
        if self.bar_providers.is_empty()
            || self.bar_providers.len() > 2
            || self.bar_providers.iter().collect::<HashSet<_>>().len() != self.bar_providers.len()
        {
            return Err("Выберите одного или двух разных провайдеров.".into());
        }
        for values in [&mut self.thresholds.claude, &mut self.thresholds.codex] {
            if values.is_empty()
                || values.len() > 5
                || values.iter().any(|v| !(1..=100).contains(v))
                || values.iter().collect::<HashSet<_>>().len() != values.len()
            {
                return Err("Укажите от 1 до 5 разных целых порогов от 1 до 100%.".into());
            }
            values.sort_unstable();
        }
        for preferences in [&mut self.preferences.claude, &mut self.preferences.codex] {
            if preferences.color != "system"
                && !(preferences.color.len() == 7
                    && preferences.color.starts_with('#')
                    && preferences.color[1..]
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit()))
            {
                return Err("Выберите системный цвет или цвет в формате #RRGGBB.".into());
            }
            if ![0, AUTO_REFRESH_MINUTES, 1, 2, 5, 10, 15, 30, 60]
                .contains(&preferences.refresh_minutes)
            {
                return Err("Выберите интервал обновления из списка.".into());
            }
            preferences.color = preferences.color.to_lowercase();
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub id: String,
    pub label: String,
    pub used_percent: f64,
    pub resets_at: Option<u64>,
    pub duration_mins: u32,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    pub id: ProviderId,
    pub label: String,
    pub status: ProviderStatus,
    pub fetched_at: Option<u64>,
    pub windows: Vec<QuotaWindow>,
}

impl ProviderSnapshot {
    /// Last observed quota is still useful after it ages or a refresh fails.
    /// Callers mark this fallback as historical; alerts use `remaining` only.
    pub fn display_remaining(&self, now: u64) -> Option<f64> {
        self.remaining(now).or_else(|| {
            if self.status == ProviderStatus::SignedOut || self.fetched_at.is_none_or(|at| at > now)
            {
                return None;
            }
            self.windows
                .iter()
                .filter(|w| {
                    w.primary
                        && w.used_percent.is_finite()
                        && (0.0..=100.0).contains(&w.used_percent)
                })
                .map(|w| 100.0 - w.used_percent)
                .reduce(f64::min)
        })
    }
    pub fn remaining(&self, now: u64) -> Option<f64> {
        if !matches!(
            self.status,
            ProviderStatus::Current | ProviderStatus::Blocked
        ) || self
            .fetched_at
            .and_then(|at| now.checked_sub(at))
            .is_none_or(|age| age > STALE_AFTER_MS)
        {
            return None;
        }
        self.windows
            .iter()
            .filter(|w| {
                w.primary
                    && w.used_percent.is_finite()
                    && (0.0..=100.0).contains(&w.used_percent)
                    && w.resets_at.is_none_or(|reset| reset > now)
            })
            .map(|w| 100.0 - w.used_percent)
            .reduce(f64::min)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    #[serde(default = "crate::i18n::system_language")]
    pub system_locale: String,
    pub connected: Vec<ProviderId>,
    pub refreshing: Option<ProviderId>,
    pub signing_in: bool,
    pub revision: u64,
    pub settings: Settings,
    pub providers: Vec<ProviderSnapshot>,
    pub notice: Option<String>,
    #[serde(default)]
    pub retry_at: RetryAt,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryAt {
    pub claude: Option<u64>,
    pub codex: Option<u64>,
}
impl RetryAt {
    pub fn get(&self, id: ProviderId) -> Option<u64> {
        match id {
            ProviderId::Claude => self.claude,
            ProviderId::Codex => self.codex,
        }
    }
    pub fn set(&mut self, id: ProviderId, value: Option<u64>) {
        match id {
            ProviderId::Claude => self.claude = value,
            ProviderId::Codex => self.codex = value,
        }
    }
}

impl AppState {
    pub fn disconnected(settings: Settings) -> Self {
        Self {
            system_locale: crate::i18n::system_language(),
            connected: vec![],
            refreshing: None,
            signing_in: false,
            revision: 0,
            settings,
            providers: [ProviderId::Claude, ProviderId::Codex]
                .into_iter()
                .map(|id| ProviderSnapshot {
                    id,
                    label: id.label().into(),
                    status: ProviderStatus::Unavailable,
                    fetched_at: None,
                    windows: vec![],
                })
                .collect(),
            notice: None,
            retry_at: RetryAt::default(),
        }
    }

    pub fn title(&self, now: u64) -> String {
        self.settings
            .bar_providers
            .iter()
            .map(|id| {
                let provider = self.providers.iter().find(|p| p.id == *id);
                let remaining = provider.and_then(|p| p.display_remaining(now));
                let historical = self.refreshing == Some(*id)
                    || provider.is_none_or(|p| p.remaining(now).is_none());
                let value = remaining.map_or_else(
                    || "—".into(),
                    |v| format!("{}{}%", if historical { "~" } else { "" }, v.round() as u8),
                );
                format!("{} {}", id.short(), value)
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    const NOW: u64 = 1_800_000_000_000;

    fn sample() -> AppState {
        let mut state = AppState::disconnected(Settings::default());
        state.connected = vec![ProviderId::Claude, ProviderId::Codex];
        for (provider, used) in state.providers.iter_mut().zip([74.0, 62.0]) {
            provider.status = ProviderStatus::Current;
            provider.fetched_at = Some(NOW);
            provider.windows = vec![
                QuotaWindow {
                    id: "session".into(),
                    label: "5 часов".into(),
                    used_percent: used,
                    resets_at: Some(NOW + 80 * 60_000),
                    duration_mins: 300,
                    primary: true,
                },
                QuotaWindow {
                    id: "week".into(),
                    label: "Неделя".into(),
                    used_percent: 18.0,
                    resets_at: None,
                    duration_mins: 10080,
                    primary: true,
                },
            ];
        }
        state
    }
    fn exhausted() -> AppState {
        let mut state = sample();
        state.providers[0].windows[0].used_percent = 96.0;
        state.providers[1].windows[0].used_percent = 100.0;
        state
    }
    #[test]
    fn updating_one_provider_keeps_the_other_counter() {
        let mut state = sample();
        state.refreshing = Some(ProviderId::Claude);
        assert_eq!(state.title(NOW), "C ~26% · X 38%");
        state.refreshing = Some(ProviderId::Codex);
        assert_eq!(state.title(NOW), "C 26% · X ~38%");
    }
    #[test]
    fn minimum_remaining_and_order_match_frontend_fixture() {
        let mut state = sample();
        assert_eq!(state.title(NOW), "C 26% · X 38%");
        state.settings.bar_providers = vec![ProviderId::Codex, ProviderId::Claude];
        assert_eq!(state.title(NOW), "X 38% · C 26%");
        state.settings.bar_providers = vec![ProviderId::Codex];
        assert_eq!(state.title(NOW), "X 38%");
    }

    #[test]
    fn critical_zero_means_exhausted_not_missing() {
        let state = exhausted();
        assert_eq!(state.title(NOW), "C 4% · X 0%");
    }

    #[test]
    fn missing_stale_signed_out_and_errors_never_become_zero() {
        for status in [
            ProviderStatus::Stale,
            ProviderStatus::Error,
            ProviderStatus::Unavailable,
            ProviderStatus::SignedOut,
        ] {
            let mut state = sample();
            for p in &mut state.providers {
                p.status = status;
            }
            assert_eq!(
                state.title(NOW),
                if status == ProviderStatus::SignedOut {
                    "C — · X —"
                } else {
                    "C ~26% · X ~38%"
                }
            );
        }
    }

    #[test]
    fn timestamps_expire_per_provider_and_future_is_rejected() {
        let mut state = sample();
        assert_eq!(state.title(NOW + STALE_AFTER_MS + 1), "C ~26% · X ~38%");
        state.providers[0].fetched_at = Some(NOW - STALE_AFTER_MS - 1);
        assert_eq!(state.title(NOW), "C ~26% · X 38%");
        state.providers[1].fetched_at = Some(NOW + 1);
        assert_eq!(state.title(NOW), "C ~26% · X —");
    }

    #[test]
    fn excludes_elapsed_invalid_and_model_specific_windows() {
        let mut state = sample();
        let provider = &mut state.providers[0];
        provider.windows[0].resets_at = Some(NOW);
        assert_eq!(provider.remaining(NOW), Some(82.0));
        for invalid in [f64::NAN, -1.0, 101.0] {
            provider.windows[1].used_percent = invalid;
            assert_eq!(provider.remaining(NOW), None);
        }
        provider.windows[1].used_percent = 90.0;
        provider.windows[1].primary = false;
        assert_eq!(provider.remaining(NOW), None);
        provider.windows.clear();
        assert_eq!(provider.remaining(NOW), None);
    }

    #[test]
    fn unknown_reset_does_not_invent_a_reset_or_discard_fresh_quota() {
        let mut state = sample();
        for window in &mut state.providers[0].windows {
            window.resets_at = None;
        }
        assert_eq!(state.providers[0].remaining(NOW), Some(26.0));
    }

    #[test]
    fn settings_are_validated_and_canonicalized() {
        let mut settings = Settings::default();
        settings.thresholds.claude = vec![100, 80, 95];
        assert_eq!(
            settings.validated().unwrap().thresholds.claude,
            vec![80, 95, 100]
        );
        for providers in [vec![], vec![ProviderId::Claude, ProviderId::Claude]] {
            assert!(Settings {
                bar_providers: providers,
                ..Settings::default()
            }
            .validated()
            .is_err());
        }
        for thresholds in [
            vec![],
            vec![0],
            vec![101],
            vec![80, 80],
            vec![1, 2, 3, 4, 5, 6],
        ] {
            let mut settings = Settings::default();
            settings.thresholds.claude = thresholds;
            assert!(settings.validated().is_err());
        }
    }

    #[test]
    fn serialization_uses_the_exact_frontend_camel_case_contract() {
        let value = serde_json::to_value(sample()).unwrap();
        assert!(value.get("mode").is_none());
        assert!(value.get("scenario").is_none());
        assert_eq!(value["connected"][0], "claude");
        assert_eq!(value["refreshing"], serde_json::Value::Null);
        assert_eq!(value["revision"], 0);
        assert_eq!(value["settings"]["barProviders"][0], "claude");
        assert_eq!(value["providers"][0]["fetchedAt"], NOW);
        assert_eq!(value["providers"][0]["windows"][0]["usedPercent"], 74.0);
        assert_eq!(
            value["providers"][0]["windows"][0]["resetsAt"],
            NOW + 80 * 60_000
        );
        assert!(value["providers"][0].get("fetched_at").is_none());
        assert!(serde_json::from_value::<AppState>(value).is_ok());
    }
    #[test]
    fn new_settings_reject_malformed_colors_intervals_and_enums() {
        for color in ["red", "#123", "#1234567", "#aabbee<script>", "#ééé"] {
            let mut s = Settings::default();
            s.preferences.claude.color = color.into();
            assert!(s.validated().is_err());
        }
        for interval in [3, 59, 61, 1440, AUTO_REFRESH_MINUTES - 1] {
            let mut s = Settings::default();
            s.preferences.codex.refresh_minutes = interval;
            assert!(s.validated().is_err());
        }
        let mut s = Settings::default();
        s.preferences.codex.refresh_minutes = AUTO_REFRESH_MINUTES;
        assert!(s.validated().is_ok());
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value["layout"] = serde_json::json!("other");
        assert!(serde_json::from_value::<Settings>(value).is_err());
    }
}
