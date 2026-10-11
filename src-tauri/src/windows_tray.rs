//! Square, antialiased provider indicators for the Windows notification area.
use crate::model::{AppState, IconStyle, IndicatorSize, ProviderId};
use std::f64::consts::{PI, TAU};

pub const ICON_SIZE: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayTheme {
    Light,
    Dark,
}

impl TrayTheme {
    fn mark_color(self) -> [u8; 3] {
        match self {
            Self::Light => [31, 35, 43],
            Self::Dark => [245, 247, 250],
        }
    }

    fn track_opacity(self) -> f64 {
        match self {
            Self::Light => 0.30,
            Self::Dark => 0.42,
        }
    }
}

/// The taskbar follows Windows mode, which can differ from the app/window theme.
#[cfg(windows)]
pub fn system_theme() -> TrayTheme {
    use windows_sys::{
        core::w,
        Win32::{
            Foundation::ERROR_SUCCESS,
            System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
        },
    };
    let mut light = 1u32;
    let mut bytes = std::mem::size_of_val(&light) as u32;
    // Read only the documented DWORD; no registry handles or theme settings are changed.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut light as *mut u32).cast(),
            &mut bytes,
        )
    };
    if result == ERROR_SUCCESS && light == 0 {
        TrayTheme::Dark
    } else {
        TrayTheme::Light
    }
}

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
    pub theme: TrayTheme,
}

impl Indicator {
    pub fn new(state: &AppState, id: ProviderId, now: u64, theme: TrayTheme) -> Self {
        let provider = state.providers.iter().find(|provider| provider.id == id);
        let remaining = provider
            .and_then(|provider| provider.display_remaining(now))
            .map(|value| value.round().clamp(0.0, 100.0) as u8);
        let historical = state.refreshing == Some(id)
            || provider.is_none_or(|provider| provider.remaining(now).is_none());
        let preferences = state.settings.preferences.get(id);
        let color = if remaining.is_none() {
            match theme {
                TrayTheme::Light => [78, 91, 109],
                TrayTheme::Dark => [190, 199, 213],
            }
        } else if let Some(color) = preferences
            .color
            .strip_prefix('#')
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        {
            [(color >> 16) as u8, (color >> 8) as u8, color as u8]
        } else {
            match (theme, remaining, id) {
                (TrayTheme::Dark, Some(0..=5), _) => [255, 132, 140],
                (TrayTheme::Dark, Some(6..=20), _) => [255, 210, 90],
                (TrayTheme::Dark, _, ProviderId::Claude) => [255, 185, 153],
                (TrayTheme::Dark, _, ProviderId::Codex) => [114, 238, 201],
                (TrayTheme::Light, Some(0..=5), _) => [184, 28, 29],
                (TrayTheme::Light, Some(6..=20), _) => [139, 83, 0],
                (TrayTheme::Light, _, ProviderId::Claude) => [163, 66, 31],
                (TrayTheme::Light, _, ProviderId::Codex) => [0, 109, 79],
            }
        };
        Self {
            id,
            remaining,
            historical,
            icon: preferences.icon,
            size: preferences.size,
            color,
            theme,
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
                let mut ring = 0.0;
                let mut mark = 0.0;
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
                        ring += self.ring_coverage(px / scale, py / scale);
                        if self.mark(px / scale, py / scale) {
                            mark += 1.0;
                        }
                    }
                }
                let pixel = ((y * size + x) * 4) as usize;
                let alpha = ring + mark;
                if alpha > 0.0 {
                    for (channel, foreground) in self.theme.mark_color().into_iter().enumerate() {
                        rgba[pixel + channel] = ((f64::from(self.color[channel]) * ring
                            + f64::from(foreground) * mark)
                            / alpha)
                            .round() as u8;
                    }
                }
                rgba[pixel + 3] = (alpha * 255.0 / 16.0).round() as u8;
            }
        }
        rgba
    }

    fn ring_coverage(&self, x: f64, y: f64) -> f64 {
        let radius = x.hypot(y);
        let angle = (y.atan2(x) + PI / 2.0).rem_euclid(TAU);
        if (10.8..=14.2).contains(&radius) {
            if (self.remaining.is_none() || self.historical) && (angle * 12.0 / TAU).fract() > 0.62
            {
                return 0.0;
            }
            let active = self
                .remaining
                .is_some_and(|value| value > 0 && angle <= f64::from(value) / 100.0 * TAU);
            return if active {
                1.0
            } else {
                self.theme.track_opacity()
            };
        }
        0.0
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
                        1.1,
                    )
                }),
                ProviderId::Codex => {
                    near_line(x, y, (-7.0, -5.5), (-1.0, 0.0), 1.15)
                        || near_line(x, y, (-1.0, 0.0), (-7.0, 5.5), 1.15)
                        || near_line(x, y, (2.0, 5.5), (7.0, 5.5), 1.15)
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
            TrayTheme::Dark,
        );
        let zero = Indicator::new(&state(0.0), ProviderId::Claude, NOW, TrayTheme::Dark);
        let current = Indicator::new(&state(62.0), ProviderId::Claude, NOW, TrayTheme::Dark);
        let old = Indicator::new(
            &state(62.0),
            ProviderId::Claude,
            NOW + STALE_AFTER_MS + 1,
            TrayTheme::Dark,
        );
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
            Indicator::new(&signed_out, ProviderId::Claude, NOW, TrayTheme::Dark).remaining,
            None
        );
    }

    #[test]
    fn ring_fills_clockwise_and_preserves_transparent_corners() {
        let mut indicator = Indicator::new(&state(25.0), ProviderId::Claude, NOW, TrayTheme::Dark);
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
        let healthy = Indicator::new(&state(80.0), ProviderId::Claude, NOW, TrayTheme::Dark);
        let warning = Indicator::new(&state(20.0), ProviderId::Claude, NOW, TrayTheme::Dark);
        let critical = Indicator::new(&state(5.0), ProviderId::Claude, NOW, TrayTheme::Dark);
        assert_ne!(healthy.color, warning.color);
        assert_ne!(warning.color, critical.color);
        let mut custom = state(80.0);
        custom.settings.preferences.claude.color = "#123456".into();
        assert_eq!(
            Indicator::new(&custom, ProviderId::Claude, NOW, TrayTheme::Dark).color,
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

    fn contrast(foreground: [u8; 3], background: [u8; 3]) -> f64 {
        let luminance = |rgb: [u8; 3]| {
            rgb.into_iter()
                .zip([0.2126, 0.7152, 0.0722])
                .map(|(channel, weight)| {
                    let c = f64::from(channel) / 255.0;
                    weight
                        * if c <= 0.04045 {
                            c / 12.92
                        } else {
                            ((c + 0.055) / 1.055).powf(2.4)
                        }
                })
                .sum::<f64>()
        };
        let a = luminance(foreground);
        let b = luminance(background);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn both_themes_keep_marks_and_quota_colors_visible_even_when_stale() {
        for (theme, background) in [
            (TrayTheme::Dark, [43, 43, 43]),
            (TrayTheme::Light, [243, 243, 243]),
        ] {
            for id in [ProviderId::Claude, ProviderId::Codex] {
                for remaining in [0.0, 5.0, 20.0, 80.0] {
                    let mut state = state(remaining);
                    state.providers[1] = state.providers[0].clone();
                    state.providers[1].id = ProviderId::Codex;
                    let current = Indicator::new(&state, id, NOW, theme);
                    let old = Indicator::new(&state, id, NOW + STALE_AFTER_MS + 1, theme);
                    assert!(contrast(current.color, background) >= 3.0);
                    assert!(contrast(theme.mark_color(), background) >= 7.0);
                    for size in [16, 32] {
                        let current_pixels = current.render(size);
                        let old_pixels = old.render(size);
                        let mut opaque_marks = 0;
                        for (new, old) in current_pixels
                            .chunks_exact(4)
                            .zip(old_pixels.chunks_exact(4))
                        {
                            if new[..3] == theme.mark_color() && new[3] == 255 {
                                opaque_marks += 1;
                                assert_eq!(new, old, "stale marks must keep their brightness");
                            }
                        }
                        assert!(opaque_marks > 0, "visible provider symbol at {size}px");
                    }
                }
            }
        }
    }

    #[test]
    fn theme_changes_rendering_and_cache_identity_with_a_custom_ring() {
        let mut state = state(62.0);
        state.settings.preferences.claude.color = "#123456".into();
        let dark = Indicator::new(&state, ProviderId::Claude, NOW, TrayTheme::Dark);
        let light = Indicator::new(&state, ProviderId::Claude, NOW, TrayTheme::Light);
        assert_eq!(dark.color, light.color);
        assert_ne!(dark, light);
        assert_ne!(dark.render(16), light.render(16));
    }
}
