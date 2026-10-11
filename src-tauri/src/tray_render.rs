//! 2x CoreText/CoreGraphics rendering at the native 22-point status-bar height.
use crate::model::{AppState, IconStyle, ProviderId, ProviderPreferences, TrayLayout};
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
pub fn render(state: &AppState, now: u64, dark: bool) -> Result<Frame, String> {
    let template = state
        .settings
        .bar_providers
        .iter()
        .all(|id| state.settings.preferences.get(*id).color == "system");
    let mut columns = Vec::new();
    let mut logical_width = 4.0;
    for id in &state.settings.bar_providers {
        let p = state.settings.preferences.get(*id);
        let scale = p.size.scale();
        let provider = state.providers.iter().find(|provider| provider.id == *id);
        let remaining = provider.and_then(|p| p.display_remaining(now));
        let historical =
            state.refreshing == Some(*id) || provider.is_none_or(|p| p.remaining(now).is_none());
        let text = remaining.map_or_else(
            || "—".into(),
            |r| format!("{}{}%", if historical { "~" } else { "" }, r.round() as u8),
        );
        let label = Label::new(
            &text,
            if state.settings.layout == TrayLayout::Stacked {
                10.0 * scale
            } else {
                12.0 * scale
            },
        )?;
        let reserved_width = Label::new(
            "~100%",
            if state.settings.layout == TrayLayout::Stacked {
                10.0 * scale
            } else {
                12.0 * scale
            },
        )?
        .width;
        let icon_width = if p.icon == IconStyle::None {
            0.0
        } else {
            12.0 * scale
        };
        let width = match state.settings.layout {
            TrayLayout::Inline => {
                icon_width
                    + if icon_width > 0.0 { 4.0 } else { 0.0 }
                    + label.width.max(reserved_width)
                    + 4.0
            }
            TrayLayout::Stacked => label.width.max(reserved_width).max(16.0 * scale) + 2.0,
            TrayLayout::Rings => 18.0 * scale + 5.0,
        };
        logical_width += width + 4.0;
        columns.push((*id, p, remaining, historical, label, width));
    }
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
    let mut x = 2.0;
    let count = columns.len();
    for (index, (id, p, remaining, historical, label, column_width)) in
        columns.into_iter().enumerate()
    {
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
                let name = (p.icon == IconStyle::None)
                    .then(|| {
                        Label::new(
                            if id == ProviderId::Claude {
                                "CLA"
                            } else {
                                "CDX"
                            },
                            7.0 * scale,
                        )
                    })
                    .transpose()?;
                // Columns keep the reserved width so the item does not jump, but a pair
                // hugs the middle: the spare room goes to the outer edges.
                let half = label
                    .width
                    .max(name.as_ref().map_or(8.0 * scale, |name| name.width))
                    / 2.0;
                let center = match (count, index) {
                    (2, 0) => x + column_width - 1.0 - half,
                    (2, _) => x + 1.0 + half,
                    _ => center,
                };
                match name {
                    Some(name) => name.draw(&ctx, center - name.width / 2.0, 14.0),
                    None => icon(&ctx, id, p.icon, center, 17.0, 8.0 * scale)?,
                }
                label.draw(&ctx, center - label.width / 2.0, 1.7);
            }
            TrayLayout::Rings => {
                let radius = 7.5 * scale;
                ctx.set_line_width(1.6 * scale);
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
        x += column_width + 4.0;
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
    use crate::model::Settings;
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
    #[test]
    fn stacked_columns_sit_close_together() {
        let mut state = AppState::disconnected(Settings::default());
        state.settings.layout = TrayLayout::Stacked;
        let frame = render(&state, 0, false).unwrap();
        let (width, height) = (frame.width as usize, frame.height as usize);
        let inked: Vec<bool> = (0..width)
            .map(|x| (0..height).any(|y| frame.rgba[(y * width + x) * 4 + 3] > 0))
            .collect();
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
