//! Export the actual native renderer with empty startup state, never fake quotas.
#[cfg(target_os = "macos")]
fn main() {
    use knowyourlimits_lib::{
        model::{AppState, Settings, TrayLayout},
        tray_render,
    };
    let Some(directory) = std::env::args_os().nth(1) else {
        eprintln!("Usage: render_tray <output-directory>");
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let mut state = AppState::disconnected(Settings::default());
    for (name, layout) in [
        ("inline", TrayLayout::Inline),
        ("stacked", TrayLayout::Stacked),
        ("rings", TrayLayout::Rings),
    ] {
        state.settings.layout = layout;
        let frame = tray_render::render(&state, 0, false).unwrap();
        let file = std::fs::File::create(directory.join(format!("{name}.png"))).unwrap();
        let mut encoder =
            png::Encoder::new(std::io::BufWriter::new(file), frame.width, frame.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&frame.rgba)
            .unwrap();
        let mut preview = frame.rgba.clone();
        for pixel in preview.chunks_exact_mut(4) {
            let alpha = u32::from(pixel[3]);
            for color in &mut pixel[..3] {
                *color = ((u32::from(*color) * alpha + 255 * (255 - alpha)) / 255) as u8;
            }
            pixel[3] = 255;
        }
        let file = std::fs::File::create(directory.join(format!("{name}-preview.png"))).unwrap();
        let mut encoder =
            png::Encoder::new(std::io::BufWriter::new(file), frame.width, frame.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&preview)
            .unwrap();
    }
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Native rendering requires macOS.");
}
