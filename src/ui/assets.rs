use eframe::egui::{ColorImage, Context, IconData, TextureHandle, TextureOptions};

const LOGO_PNG: &[u8] = include_bytes!("../../assets/crashlens-logo.png");

fn decoded_logo() -> Option<(Vec<u8>, u32, u32)> {
    let image = image::load_from_memory(LOGO_PNG).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some((image.into_raw(), width, height))
}

pub fn app_icon() -> Option<IconData> {
    let (rgba, width, height) = decoded_logo()?;
    Some(IconData {
        rgba,
        width,
        height,
    })
}

pub fn logo_texture(context: &Context) -> TextureHandle {
    let image = decoded_logo()
        .map(|(rgba, width, height)| {
            ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba)
        })
        .unwrap_or_else(|| ColorImage::new([1, 1], vec![eframe::egui::Color32::TRANSPARENT]));
    context.load_texture("crashlens_official_logo", image, TextureOptions::LINEAR)
}

#[cfg(test)]
mod tests {
    use super::decoded_logo;

    #[test]
    fn embedded_logo_is_a_nonempty_square_rgba_image() {
        let (pixels, width, height) = decoded_logo().expect("embedded logo must decode");
        assert_eq!(width, height);
        assert!(width >= 512);
        assert_eq!(pixels.len(), (width * height * 4) as usize);
    }
}
