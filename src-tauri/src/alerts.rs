use crate::i18n::t;
use crate::model::{ProviderId, ProviderSnapshot, ProviderStatus, Settings};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Alert {
    pub provider: ProviderId,
    pub title: String,
    pub body: String,
    pub sound: bool,
}
#[derive(Default)]
struct WindowAlert {
    cycle: Option<u64>,
    highest: u8,
    warned: bool,
}
#[derive(Default)]
pub struct AlertTracker {
    windows: HashMap<ProviderId, HashMap<String, WindowAlert>>,
}
impl AlertTracker {
    pub fn reset(&mut self, id: ProviderId) {
        self.windows.remove(&id);
    }
    pub fn observe(
        &mut self,
        snapshot: &ProviderSnapshot,
        settings: &Settings,
        now: u64,
    ) -> Option<Alert> {
        let options = &settings.preferences.get(snapshot.id).notifications;
        if !options.enabled
            || snapshot.status != ProviderStatus::Current
            || snapshot.remaining(now).is_none()
        {
            return None;
        }
        let thresholds = settings.thresholds.get(snapshot.id);
        let first = *thresholds.first()?;
        let windows = self.windows.entry(snapshot.id).or_default();
        let valid: Vec<_> = snapshot
            .windows
            .iter()
            .filter(|w| {
                w.primary
                    && w.used_percent.is_finite()
                    && (0.0..=100.0).contains(&w.used_percent)
                    && w.resets_at.is_none_or(|at| at > now)
            })
            .collect();
        let previously_warned: Vec<_> = windows
            .iter()
            .filter(|(_, v)| v.warned)
            .map(|(key, _)| key.clone())
            .collect();
        let recovered = !previously_warned.is_empty()
            && previously_warned.iter().all(|key| {
                valid
                    .iter()
                    .any(|w| &w.id == key && w.used_percent < f64::from(first))
            })
            && valid.iter().all(|w| w.used_percent < f64::from(first));
        let mut crossed = 0;
        for window in &valid {
            let cycle = window.resets_at.map(|at| at / 60_000);
            let entry = windows.entry(window.id.clone()).or_default();
            if entry.cycle != cycle {
                entry.highest = 0;
                entry.cycle = cycle;
            }
            if window.used_percent < f64::from(first) {
                entry.highest = 0;
                entry.warned = false;
            } else if let Some(level) = thresholds
                .iter()
                .copied()
                .filter(|level| f64::from(*level) <= window.used_percent)
                .max()
            {
                if level > entry.highest {
                    crossed = crossed.max(level);
                    entry.highest = level;
                }
                entry.warned = true;
            }
        }
        // Removed or elapsed windows cannot prove recovery. Retain their history
        // but bound it to the primary windows present in the next valid read.
        windows.retain(|key, _| snapshot.windows.iter().any(|w| &w.id == key));
        let remaining = snapshot.remaining(now)?;
        let title = if crossed > 0 {
            t(
                &settings.language,
                "alertThreshold",
                &[
                    ("provider", snapshot.id.label()),
                    ("percent", &crossed.to_string()),
                ],
            )
        } else if recovered && options.recovery {
            t(
                &settings.language,
                "alertRecovery",
                &[("provider", snapshot.id.label())],
            )
        } else {
            return None;
        };
        Some(Alert {
            provider: snapshot.id,
            title,
            body: t(
                &settings.language,
                "alertRemaining",
                &[("percent", &(remaining.round() as u8).to_string())],
            ),
            sound: options.sound,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::QuotaWindow;
    const NOW: u64 = 1_800_000_000_000;
    fn snapshot(used: f64) -> ProviderSnapshot {
        ProviderSnapshot {
            id: ProviderId::Claude,
            label: "Claude".into(),
            status: ProviderStatus::Current,
            fetched_at: Some(NOW),
            windows: vec![QuotaWindow {
                id: "session".into(),
                label: "5 часов".into(),
                used_percent: used,
                resets_at: Some(NOW + 3_600_000),
                duration_mins: 300,
                primary: true,
            }],
        }
    }
    fn settings() -> Settings {
        let mut s = Settings {
            language: "en".into(),
            ..Settings::default()
        };
        s.preferences.claude.notifications.enabled = true;
        s.preferences.claude.notifications.recovery = true;
        s
    }
    #[test]
    fn thresholds_coalesce_deduplicate_and_cross_independently() {
        let mut t = AlertTracker::default();
        let s = settings();
        assert!(t.observe(&snapshot(79.0), &s, NOW).is_none());
        assert!(t
            .observe(&snapshot(96.0), &s, NOW)
            .unwrap()
            .title
            .contains("95%"));
        assert!(t.observe(&snapshot(97.0), &s, NOW).is_none());
        assert!(t
            .observe(&snapshot(100.0), &s, NOW)
            .unwrap()
            .title
            .contains("100%"));
    }
    #[test]
    fn stale_error_elapsed_future_and_disabled_do_not_alert() {
        for status in [
            ProviderStatus::Stale,
            ProviderStatus::Error,
            ProviderStatus::Unavailable,
        ] {
            let mut p = snapshot(100.0);
            p.status = status;
            assert!(AlertTracker::default()
                .observe(&p, &settings(), NOW)
                .is_none());
        }
        let mut p = snapshot(100.0);
        p.windows[0].resets_at = Some(NOW);
        assert!(AlertTracker::default()
            .observe(&p, &settings(), NOW)
            .is_none());
        let mut p = snapshot(100.0);
        p.fetched_at = Some(NOW + 1);
        assert!(AlertTracker::default()
            .observe(&p, &settings(), NOW)
            .is_none());
        assert!(AlertTracker::default()
            .observe(&snapshot(100.0), &Settings::default(), NOW)
            .is_none());
    }
    #[test]
    fn fractional_reset_jitter_does_not_repeat_but_new_cycle_does() {
        let mut t = AlertTracker::default();
        let s = settings();
        let mut p = snapshot(85.0);
        assert!(t.observe(&p, &s, NOW).is_some());
        p.windows[0].resets_at = Some(NOW + 3_600_500);
        assert!(t.observe(&p, &s, NOW).is_none());
        p.windows[0].resets_at = Some(NOW + 7_200_000);
        assert!(t.observe(&p, &s, NOW).is_some());
    }
    #[test]
    fn recovery_requires_new_valid_read_and_only_emits_once() {
        let mut t = AlertTracker::default();
        let s = settings();
        t.observe(&snapshot(95.0), &s, NOW);
        let mut p = snapshot(20.0);
        p.status = ProviderStatus::Stale;
        assert!(t.observe(&p, &s, NOW).is_none());
        p.status = ProviderStatus::Current;
        assert!(t
            .observe(&p, &s, NOW)
            .unwrap()
            .title
            .contains("quota restored"));
        assert!(t.observe(&p, &s, NOW).is_none());
    }
    #[test]
    fn provider_settings_sound_and_thresholds_are_independent() {
        let mut t = AlertTracker::default();
        let mut s = settings();
        s.preferences.claude.notifications.sound = true;
        let alert = t.observe(&snapshot(95.0), &s, NOW).unwrap();
        assert!(alert.sound);
        let mut p = snapshot(100.0);
        p.id = ProviderId::Codex;
        assert!(t.observe(&p, &s, NOW).is_none());
        s.preferences.codex.notifications.enabled = true;
        s.thresholds.codex = vec![99];
        let alert = t.observe(&p, &s, NOW).unwrap();
        assert!(!alert.sound);
        assert!(alert.title.contains("99%"));
    }
    #[test]
    fn missing_warned_window_does_not_invent_recovery() {
        let mut t = AlertTracker::default();
        let s = settings();
        t.observe(&snapshot(95.0), &s, NOW);
        let mut p = snapshot(20.0);
        p.windows[0].id = "weekly".into();
        assert!(t.observe(&p, &s, NOW).is_none());
    }
}
