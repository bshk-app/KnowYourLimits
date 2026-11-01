//! 2x CoreText/CoreGraphics rendering at the native 22-point status-bar height.
use crate::model::{
    AppState, IconStyle, ProviderId, ProviderPreferences, ProviderSnapshot, ProviderStatus,
    TrayLayout,
};
use core_graphics::{
    base::kCGImageAlphaPremultipliedLast,
    color_space::CGColorSpace,
    context::{CGContext, CGLineCap},
    geometry::{CGPoint, CGRect, CGSize},
};
use core_text::{
    font::{self, CTFont},
    font_descriptor::kCTFontOrientationHorizontal,
};
use std::f64::consts::PI;

/// Points of transparent space at each end; the menu bar adds its own spacing.
const EDGE: f64 = 1.0;
/// Points between provider columns.
const GAP: f64 = 4.0;
const RING_RADIUS: f64 = 7.5;
const RING_LINE: f64 = 1.6;

pub struct Frame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub logical_width: f64,
    pub template: bool,
}
struct Label {
    font: CTFont,
    glyphs: Vec<u16>,
    positions: Vec<CGPoint>,
    width: f64,
}
impl Label {
    fn new(text: &str, size: f64) -> Result<Self, String> {
        let font = font::new_from_name(".AppleSystemUIFont", size)
            .or_else(|_| font::new_from_name("Helvetica", size))
            .map_err(|_| "Не удалось загрузить системный шрифт.".to_owned())?;
        let characters: Vec<u16> = text.encode_utf16().collect();
        let mut glyphs = vec![0; characters.len()];
        let mut advances = vec![CGSize::new(0.0, 0.0); characters.len()];
        // SAFETY: All buffers have characters.len() elements and are valid for
        // the synchronous CoreText calls; only our fixed labels/numbers enter.
        unsafe {
            font.get_glyphs_for_characters(
                characters.as_ptr(),
                glyphs.as_mut_ptr(),
                characters.len() as _,
            );
            font.get_advances_for_glyphs(
                kCTFontOrientationHorizontal,
                glyphs.as_ptr(),
                advances.as_mut_ptr(),
                glyphs.len() as _,
            );
        }
        let mut x = 0.0;
        let positions = advances
            .iter()
            .map(|advance| {
                let point = CGPoint::new(x, 0.0);
                x += advance.width;
                point
            })
            .collect();
        Ok(Self {
            font,
            glyphs,
            positions,
            width: x,
        })
    }
    fn draw(&self, ctx: &CGContext, x: f64, y: f64) {
        let positions: Vec<_> = self
            .positions
            .iter()
            .map(|p| CGPoint::new(p.x + x, y))
            .collect();
        self.font.draw_glyphs(&self.glyphs, &positions, ctx.clone());
    }
}
fn color(preferences: &ProviderPreferences, dark: bool, template: bool) -> (f64, f64, f64) {
    if template {
        return (0.0, 0.0, 0.0);
    }
    if preferences.color == "system" {
        return if dark {
            (0.94, 0.94, 0.94)
        } else {
            (0.12, 0.12, 0.12)
        };
    }
    let value = u32::from_str_radix(&preferences.color[1..], 16).unwrap_or(0);
    (
        f64::from((value >> 16) & 255) / 255.0,
        f64::from((value >> 8) & 255) / 255.0,
        f64::from(value & 255) / 255.0,
    )
}
fn line(ctx: &CGContext, x: f64, y: f64, points: &[(f64, f64)], scale: f64) {
    for (i, (px, py)) in points.iter().enumerate() {
        if i == 0 {
            ctx.move_to_point(x + px * scale, y + py * scale);
        } else {
            ctx.add_line_to_point(x + px * scale, y + py * scale);
        }
    }
    ctx.stroke_path();
}
fn icon(
    ctx: &CGContext,
    id: ProviderId,
    kind: IconStyle,
    cx: f64,
    cy: f64,
    size: f64,
) -> Result<(), String> {
    ctx.set_line_cap(CGLineCap::CGLineCapRound);
    ctx.set_line_width(1.2);
    match kind {
        IconStyle::None => {}
        IconStyle::Dot => ctx.fill_ellipse_in_rect(CGRect::new(
            &CGPoint::new(cx - size * 0.18, cy - size * 0.18),
            &CGSize::new(size * 0.36, size * 0.36),
        )),
        IconStyle::Letter => {
            let letter = Label::new(id.short(), size)?;
            letter.draw(ctx, cx - letter.width / 2.0, cy - size * 0.36);
        }
        IconStyle::Symbol => match id {
            ProviderId::Claude => {
                for i in 0..12 {
                    let angle = f64::from(i) * PI / 6.0;
                    line(
                        ctx,
                        cx,
                        cy,
                        &[
                            (angle.cos() * 0.17, angle.sin() * 0.17),
                            (angle.cos() * 0.48, angle.sin() * 0.48),
                        ],
                        size,
                    );
                }
            }
            ProviderId::Codex => {
                line(
                    ctx,
                    cx,
                    cy,
                    &[(-0.45, -0.33), (-0.05, 0.0), (-0.45, 0.33)],
                    size,
                );
                line(ctx, cx, cy, &[(0.10, -0.33), (0.47, -0.33)], size);
            }
        },
    }
    Ok(())
}
fn arc(ctx: &CGContext, cx: f64, cy: f64, r: f64, fraction: f64) {
    let count = (fraction * 64.0).ceil().max(1.0) as usize;
    for i in 0..=count {
        let a = PI / 2.0 - (i as f64 / count as f64) * fraction * 2.0 * PI;
        let point = CGPoint::new(cx + r * a.cos(), cy + r * a.sin());
        if i == 0 {
            ctx.move_to_point(point.x, point.y);
        } else {
            ctx.add_line_to_point(point.x, point.y);
        }
    }
    ctx.stroke_path();
}
/// Per bar provider: displayed remaining percent, stale, refreshing. Besides
/// settings and theme, these are the only frame inputs, so they key the tray cache.
pub fn readings(state: &AppState, now: u64) -> Vec<(Option<f64>, bool, bool)> {
    state
        .settings
        .bar_providers
        .iter()
        .map(|id| {
            let refreshing = state.refreshing == Some(*id);
            // A refresh marks its snapshot Stale before any new reading exists; judge
            // it as current so neither the value nor its `~` changes until it ends.
            let provider = state
                .providers
                .iter()
                .find(|p| p.id == *id)
                .map(|p| ProviderSnapshot {
                    status: if refreshing {
                        ProviderStatus::Current
                    } else {
                        p.status
                    },
                    ..p.clone()
                });
            (
                provider.as_ref().and_then(|p| p.display_remaining(now)),
                provider.is_none_or(|p| p.remaining(now).is_none()),
                refreshing,
            )
        })
        .collect()
}
pub fn render(state: &AppState, now: u64, dark: bool) -> Result<Frame, String> {
    let template = state
        .settings
        .bar_providers
        .iter()
        .all(|id| state.settings.preferences.get(*id).color == "system");
    let mut columns = Vec::new();
    for (id, (remaining, stale, refreshing)) in state
        .settings
        .bar_providers
        .iter()
        .zip(readings(state, now))
    {
        let p = state.settings.preferences.get(*id);
        let scale = p.size.scale();
        // A refresh only dims the value: a temporary `~` would resize the item
        // and shift its menu bar neighbours.
        let historical = stale || refreshing;
        let text = remaining.map_or_else(
            || "—".into(),
            |r| format!("{}{}%", if stale { "~" } else { "" }, r.round() as u8),
        );
        let label = Label::new(
            &text,
            if state.settings.layout == TrayLayout::Stacked {
                10.0 * scale
            } else {
                12.0 * scale
            },
        )?;
        let name = (state.settings.layout == TrayLayout::Stacked && p.icon == IconStyle::None)
            .then(|| {
                Label::new(
                    if *id == ProviderId::Claude {
                        "CLA"
                    } else {
                        "CDX"
                    },
                    7.0 * scale,
                )
            })
            .transpose()?;
        let icon_width = if p.icon == IconStyle::None {
            0.0
        } else {
            12.0 * scale
        };
        let width = match state.settings.layout {
            TrayLayout::Inline => {
                icon_width + if icon_width > 0.0 { 4.0 } else { 0.0 } + label.width
            }
            TrayLayout::Stacked => label
                .width
                .max(name.as_ref().map_or(8.0 * scale, |name| name.width)),
            TrayLayout::Rings => (2.0 * RING_RADIUS + RING_LINE) * scale,
        };
        columns.push((*id, p, remaining, historical, label, name, width));
    }
    let logical_width = 2.0 * EDGE
        + columns.iter().map(|column| column.6).sum::<f64>()
        + GAP * columns.len().saturating_sub(1) as f64;
    let width = (logical_width * 2.0).ceil() as usize;
    let height = 44;
    let mut ctx = CGContext::create_bitmap_context(
        None,
        width,
        height,
        8,
        width * 4,
        &CGColorSpace::create_device_rgb(),
        kCGImageAlphaPremultipliedLast,
    );
    ctx.clear_rect(CGRect::new(
        &CGPoint::new(0.0, 0.0),
        &CGSize::new(width as f64, height as f64),
    ));
    ctx.scale(2.0, 2.0);
    let mut x = EDGE;
    for (id, p, remaining, historical, label, name, column_width) in columns {
        let (r, g, b) = color(p, dark, template);
        let alpha = if remaining.is_none() {
            0.52
        } else if historical {
            0.65
        } else {
            1.0
        };
        ctx.set_rgb_fill_color(r, g, b, alpha);
        ctx.set_rgb_stroke_color(r, g, b, alpha);
        let scale = p.size.scale();
        let center = x + column_width / 2.0;
        match state.settings.layout {
            TrayLayout::Inline => {
                let icon_width = if p.icon == IconStyle::None {
                    0.0
                } else {
                    12.0 * scale
                };
                icon(&ctx, id, p.icon, x + icon_width / 2.0, 11.0, icon_width)?;
                label.draw(
                    &ctx,
                    x + icon_width + if icon_width > 0.0 { 4.0 } else { 0.0 },
                    6.5,
                );
            }
            TrayLayout::Stacked => {
                match name {
                    Some(name) => name.draw(&ctx, center - name.width / 2.0, 14.0),
                    None => icon(&ctx, id, p.icon, center, 17.0, 8.0 * scale)?,
                }
                label.draw(&ctx, center - label.width / 2.0, 1.7);
            }
            TrayLayout::Rings => {
                let radius = RING_RADIUS * scale;
                ctx.set_line_width(RING_LINE * scale);
                ctx.set_rgb_stroke_color(r, g, b, 0.22 * alpha);
                arc(&ctx, center, 11.0, radius, 1.0);
                if let Some(remaining) = remaining.filter(|value| *value > 0.0) {
                    ctx.set_rgb_stroke_color(r, g, b, alpha);
                    arc(&ctx, center, 11.0, radius, remaining / 100.0);
                }
                ctx.set_rgb_stroke_color(r, g, b, alpha);
                icon(&ctx, id, p.icon, center, 11.0, 8.0 * scale)?;
            }
        }
        x += column_width + GAP;
    }
    let mut rgba = ctx.data().to_vec();
    // CGBitmap is premultiplied; Tauri's RGBA image contract is straight alpha.
    for pixel in rgba.chunks_exact_mut(4) {
        let a = u32::from(pixel[3]);
        if a > 0 {
            for channel in &mut pixel[..3] {
                *channel = (u32::from(*channel) * 255 / a).min(255) as u8;
            }
        }
    }
    Ok(Frame {
        rgba,
        width: width as u32,
        height: height as u32,
        logical_width: width as f64 / 2.0,
        template,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ProviderSnapshot, ProviderStatus, QuotaWindow, Settings, STALE_AFTER_MS};
    #[test]
    fn every_layout_is_retina_sized_bounded_and_visible_without_fake_quota() {
        let mut state = AppState::disconnected(Settings::default());
        let mut widths = Vec::new();
        for layout in [TrayLayout::Inline, TrayLayout::Stacked, TrayLayout::Rings] {
            state.settings.layout = layout;
            let frame = render(&state, 0, false).unwrap();
            assert_eq!(frame.height, 44);
            assert_eq!(frame.logical_width * 2.0, f64::from(frame.width));
            assert_eq!(frame.rgba.len(), (frame.width * frame.height * 4) as usize);
            assert!(frame.width < 280);
            assert!(frame.rgba.chunks_exact(4).any(|pixel| pixel[3] > 0));
            assert!(frame.template);
            widths.push(frame.width);
        }
        assert_ne!(widths[0], widths[2]);
    }
    #[test]
    fn provider_color_and_size_affect_real_raster_and_disable_template_tinting() {
        let mut state = AppState::disconnected(Settings::default());
        let before = render(&state, 0, false).unwrap();
        state.settings.preferences.claude.color = "#ff6600".into();
        state.settings.preferences.claude.size = crate::model::IndicatorSize::Large;
        let after = render(&state, 0, false).unwrap();
        assert!(!after.template);
        assert_ne!(before.rgba, after.rgba);
        assert!(after.width > before.width);
    }
    const NOW: u64 = 1_800_000_000_000;
    fn inked(frame: &Frame) -> Vec<bool> {
        let (width, height) = (frame.width as usize, frame.height as usize);
        (0..width)
            .map(|x| (0..height).any(|y| frame.rgba[(y * width + x) * 4 + 3] > 0))
            .collect()
    }
    fn showing(layout: TrayLayout, refreshing: Option<ProviderId>, fetched_at: u64) -> Frame {
        let mut state = AppState::disconnected(Settings::default());
        state.settings.layout = layout;
        state.refreshing = refreshing;
        state.providers = [(ProviderId::Claude, 31.0), (ProviderId::Codex, 21.0)]
            .map(|(id, used_percent)| ProviderSnapshot {
                id,
                label: id.label().into(),
                // Like Coordinator::start, a refresh marks its snapshot Stale.
                status: if refreshing == Some(id) {
                    ProviderStatus::Stale
                } else {
                    ProviderStatus::Current
                },
                fetched_at: Some(fetched_at),
                windows: vec![QuotaWindow {
                    id: "weekly".into(),
                    label: "Week".into(),
                    used_percent,
                    resets_at: None,
                    duration_mins: 10080,
                    primary: true,
                }],
            })
            .into();
        render(&state, NOW, false).unwrap()
    }
    #[test]
    fn indicator_leaves_at_most_two_points_around_its_content() {
        for layout in [TrayLayout::Inline, TrayLayout::Stacked, TrayLayout::Rings] {
            let inked = inked(&showing(layout, None, NOW));
            let left = inked.iter().position(|&ink| ink).unwrap();
            let right = inked.iter().rev().position(|&ink| ink).unwrap();
            assert!(
                left <= 4 && right <= 4,
                "{layout:?}: {left} px left, {right} px right"
            );
        }
    }
    #[test]
    fn refresh_dims_values_without_resizing_while_stale_values_keep_tilde() {
        for layout in [TrayLayout::Inline, TrayLayout::Stacked] {
            let current = showing(layout, None, NOW);
            let refreshing = showing(layout, Some(ProviderId::Claude), NOW);
            assert_eq!(refreshing.width, current.width, "{layout:?}");
            assert!(refreshing.rgba != current.rgba, "{layout:?}");
            let old = NOW - STALE_AFTER_MS - 1;
            let stale = showing(layout, None, old);
            assert!(stale.width > current.width, "{layout:?}");
            let stale_refreshing = showing(layout, Some(ProviderId::Claude), old);
            assert_eq!(stale_refreshing.width, stale.width, "{layout:?}");
        }
    }
    #[test]
    fn readings_change_whenever_the_frame_does() {
        let mut stale = AppState::disconnected(Settings::default());
        stale.providers = vec![ProviderSnapshot {
            id: ProviderId::Claude,
            label: "Claude".into(),
            status: ProviderStatus::Current,
            fetched_at: Some(NOW - STALE_AFTER_MS - 1),
            windows: vec![QuotaWindow {
                id: "weekly".into(),
                label: "Week".into(),
                used_percent: 31.0,
                resets_at: None,
                duration_mins: 10080,
                primary: true,
            }],
        }];
        let mut refreshing = stale.clone();
        refreshing.providers[0].fetched_at = Some(NOW);
        refreshing.providers[0].status = ProviderStatus::Stale;
        refreshing.refreshing = Some(ProviderId::Claude);
        assert!(
            render(&stale, NOW, false).unwrap().rgba
                != render(&refreshing, NOW, false).unwrap().rgba
        );
        assert_ne!(readings(&stale, NOW), readings(&refreshing, NOW));
    }
    #[test]
    fn refresh_keeps_the_value_and_tilde_shown_before_it() {
        let mut current = AppState::disconnected(Settings::default());
        let window = |used_percent, resets_at| QuotaWindow {
            id: format!("{used_percent}"),
            label: "Window".into(),
            used_percent,
            resets_at,
            duration_mins: 300,
            primary: true,
        };
        current.providers = vec![ProviderSnapshot {
            id: ProviderId::Claude,
            label: "Claude".into(),
            status: ProviderStatus::Current,
            fetched_at: Some(NOW),
            // An elapsed window must not reappear once the refresh starts.
            windows: vec![window(99.0, Some(NOW - 1)), window(18.0, None)],
        }];
        let mut refreshing = current.clone();
        refreshing.providers[0].status = ProviderStatus::Stale;
        refreshing.refreshing = Some(ProviderId::Claude);
        let (before, after) = (readings(&current, NOW), readings(&refreshing, NOW));
        assert_eq!((after[0].0, after[0].1), (before[0].0, before[0].1));
    }
    #[test]
    fn stacked_columns_sit_close_together() {
        let mut state = AppState::disconnected(Settings::default());
        state.settings.layout = TrayLayout::Stacked;
        let frame = render(&state, 0, false).unwrap();
        let inked = inked(&frame);
        let first = inked.iter().position(|&ink| ink).unwrap();
        let last = inked.iter().rposition(|&ink| ink).unwrap();
        let widest_gap = inked[first..=last]
            .split(|&ink| ink)
            .map(<[bool]>::len)
            .max()
            .unwrap();
        // Retina pixels: the two columns are at most 8 pt apart.
        assert!(
            widest_gap <= 16,
            "stacked columns are {widest_gap} px apart"
        );
    }
}
