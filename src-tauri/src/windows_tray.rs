//! Square, antialiased provider indicators for the Windows notification area.
use crate::model::{AppState, IconStyle, IndicatorSize, ProviderId};
use std::f64::consts::{PI, TAU};

pub const ICON_SIZE: u32 = 32;

pub fn tray_id(id: ProviderId) -> &'static str {
    match id {
        ProviderId::Claude => "claude-tray",
        ProviderId::Codex => "codex-tray",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Indicator {
    pub id: ProviderId,
    pub remaining: Option<u8>,
    pub historical: bool,
    pub icon: IconStyle,
    pub size: IndicatorSize,
    pub color: [u8; 3],
}

impl Indicator {
    pub fn new(state: &AppState, id: ProviderId, now: u64) -> Self {
        let provider = state.providers.iter().find(|provider| provider.id == id);
        let remaining = provider
            .and_then(|provider| provider.display_remaining(now))
            .map(|value| value.round().clamp(0.0, 100.0) as u8);
        let historical = state.refreshing == Some(id)
            || provider.is_none_or(|provider| provider.remaining(now).is_none());
        let preferences = state.settings.preferences.get(id);
        let color = if remaining.is_none() {
            [148, 156, 170]
        } else if let Some(color) = preferences
            .color
            .strip_prefix('#')
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        {
            [(color >> 16) as u8, (color >> 8) as u8, color as u8]
        } else {
            match remaining {
                Some(0..=5) => [244, 83, 83],
                Some(6..=20) => [245, 180, 55],
                _ => match id {
                    ProviderId::Claude => [223, 136, 104],
                    ProviderId::Codex => [75, 197, 156],
                },
            }
        };
        Self {
            id,
            remaining,
            historical,
            icon: preferences.icon,
            size: preferences.size,
            color,
        }
    }

    pub fn tooltip(&self) -> String {
        let value = self.remaining.map_or_else(
            || "—".into(),
            |remaining| format!("{}{remaining}%", if self.historical { "~" } else { "" }),
        );
        format!("KnowYourLimits\n{} {value}", self.id.label())
    }

    /// Render directly to straight-alpha RGBA; no fonts or platform drawing APIs.
    pub fn render(&self, size: u32) -> Vec<u8> {
        let mut rgba = vec![0; (size * size * 4) as usize];
        for y in 0..size {
            for x in 0..size {
                let mut alpha = 0.0;
                // Supersampling preserves narrow strokes at the normal 16-pixel tray size.
                for sy in 0..4 {
                    for sx in 0..4 {
                        let px = (f64::from(x) + (f64::from(sx) + 0.5) / 4.0) * 32.0
                            / f64::from(size)
                            - 16.0;
                        let py = (f64::from(y) + (f64::from(sy) + 0.5) / 4.0) * 32.0
                            / f64::from(size)
                            - 16.0;
                        let scale = self.size.scale() / 1.15;
                        alpha += self.coverage(px / scale, py / scale);
                    }
                }
                let pixel = ((y * size + x) * 4) as usize;
                rgba[pixel..pixel + 3].copy_from_slice(&self.color);
                rgba[pixel + 3] = (alpha * 255.0 / 16.0).round() as u8;
            }
        }
        rgba
    }

    fn coverage(&self, x: f64, y: f64) -> f64 {
        let radius = x.hypot(y);
        let angle = (y.atan2(x) + PI / 2.0).rem_euclid(TAU);
        let opacity = if self.historical { 0.65 } else { 1.0 };
        if (11.4..=14.2).contains(&radius) {
            if (self.remaining.is_none() || self.historical) && (angle * 12.0 / TAU).fract() > 0.62
            {
                return 0.0;
            }
            let active = self
                .remaining
                .is_some_and(|value| value > 0 && angle <= f64::from(value) / 100.0 * TAU);
            return if active { opacity } else { 0.26 };
        }
        if self.mark(x, y) {
            opacity
        } else {
            0.0
        }
    }

    fn mark(&self, x: f64, y: f64) -> bool {
        match self.icon {
            IconStyle::None => false,
            IconStyle::Dot => x.hypot(y) <= 2.5,
            IconStyle::Letter => {
                let columns = match self.id {
                    ProviderId::Claude => [0b11111, 0b10001, 0b10001],
                    ProviderId::Codex => [0b11011, 0b00100, 0b11011],
                };
                let column = ((x + 4.5) / 3.0).floor() as i32;
                let row = ((y + 7.5) / 3.0).floor() as i32;
                (0..3).contains(&column)
                    && (0..5).contains(&row)
                    && columns[column as usize] & (1 << (4 - row)) != 0
            }
            IconStyle::Symbol => match self.id {
                ProviderId::Claude => (0..12).any(|ray| {
                    let angle = f64::from(ray) * TAU / 12.0;
                    near_line(
                        x,
                        y,
                        (angle.cos() * 2.4, angle.sin() * 2.4),
                        (angle.cos() * 7.5, angle.sin() * 7.5),
                        0.8,
                    )
                }),
                ProviderId::Codex => {
                    near_line(x, y, (-7.0, -5.5), (-1.0, 0.0), 1.0)
                        || near_line(x, y, (-1.0, 0.0), (-7.0, 5.5), 1.0)
                        || near_line(x, y, (2.0, 5.5), (7.0, 5.5), 1.0)
                }
            },
        }
    }
}

fn near_line(x: f64, y: f64, start: (f64, f64), end: (f64, f64), radius: f64) -> bool {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let t = (((x - start.0) * dx + (y - start.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (x - start.0 - t * dx).hypot(y - start.1 - t * dy) <= radius
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ProviderStatus, QuotaWindow, Settings, STALE_AFTER_MS};
    const NOW: u64 = 1_800_000_000_000;

    fn state(remaining: f64) -> AppState {
        let mut state = AppState::disconnected(Settings::default());
        let provider = &mut state.providers[0];
        provider.status = ProviderStatus::Current;
        provider.fetched_at = Some(NOW);
        provider.windows = vec![QuotaWindow {
            id: "session".into(),
            label: "Session".into(),
            used_percent: 100.0 - remaining,
            resets_at: Some(NOW + 3_600_000),
            duration_mins: 300,
            primary: true,
        }];
        state
    }

    #[test]
    fn unknown_exhausted_and_historical_quotas_are_distinct() {
        let unknown = Indicator::new(
            &AppState::disconnected(Settings::default()),
            ProviderId::Claude,
            NOW,
        );
        let zero = Indicator::new(&state(0.0), ProviderId::Claude, NOW);
        let current = Indicator::new(&state(62.0), ProviderId::Claude, NOW);
        let old = Indicator::new(&state(62.0), ProviderId::Claude, NOW + STALE_AFTER_MS + 1);
        assert_eq!(unknown.remaining, None);
        assert_eq!(zero.remaining, Some(0));
        assert_ne!(unknown.render(16), zero.render(16));
        assert_ne!(old.render(16), current.render(16));
        assert!(old.tooltip().contains("~62%"));
        assert!(unknown.tooltip().contains('—'));
        assert!(!zero.historical);
        let mut signed_out = state(62.0);
        signed_out.providers[0].status = ProviderStatus::SignedOut;
        assert_eq!(
            Indicator::new(&signed_out, ProviderId::Claude, NOW).remaining,
            None
        );
    }

    #[test]
    fn ring_fills_clockwise_and_preserves_transparent_corners() {
        let mut indicator = Indicator::new(&state(25.0), ProviderId::Claude, NOW);
        indicator.icon = IconStyle::None;
        indicator.size = IndicatorSize::Large;
        let quarter = indicator.render(32);
        indicator.remaining = Some(100);
        let full = indicator.render(32);
        let opacity = |image: &[u8]| {
            image
                .chunks_exact(4)
                .map(|pixel| u32::from(pixel[3]))
                .sum::<u32>()
        };
        assert!(opacity(&full) > opacity(&quarter));
        assert_eq!(full[3], 0);
        assert_eq!(full[((16 * 32 + 16) * 4 + 3) as usize], 0);
        assert!(
            quarter[((6 * 32 + 24) * 4 + 3) as usize] > quarter[((24 * 32 + 6) * 4 + 3) as usize]
        );
    }

    #[test]
    fn warnings_custom_colors_and_provider_symbols_are_respected() {
        let healthy = Indicator::new(&state(80.0), ProviderId::Claude, NOW);
        let warning = Indicator::new(&state(20.0), ProviderId::Claude, NOW);
        let critical = Indicator::new(&state(5.0), ProviderId::Claude, NOW);
        assert_ne!(healthy.color, warning.color);
        assert_ne!(warning.color, critical.color);
        let mut custom = state(80.0);
        custom.settings.preferences.claude.color = "#123456".into();
        assert_eq!(
            Indicator::new(&custom, ProviderId::Claude, NOW).color,
            [0x12, 0x34, 0x56]
        );
        let mut other = healthy.clone();
        other.id = ProviderId::Codex;
        assert_ne!(healthy.render(16), other.render(16));
        for size in [16, 32, 48] {
            let pixels = healthy.render(size);
            assert_eq!(pixels.len(), (size * size * 4) as usize);
            assert!(pixels
                .chunks_exact(4)
                .any(|pixel| (1..255).contains(&pixel[3])));
        }
    }
}
