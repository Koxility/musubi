//! The Musubi logo. There is one source, `packaging/musubi.svg`, which the launcher entry
//! installs as well; the sidebar, the About line and the window icon all draw it.

use std::sync::LazyLock;

use iced::Element;
use iced::widget::svg;
use iced::window;
use resvg::{tiny_skia, usvg};

const SVG: &[u8] = include_bytes!("../../../packaging/musubi.svg");
/// Edge of the window icon in pixels, large enough for task switchers.
const ICON_SIZE: u32 = 256;

/// The logo as a square of `size` pixels.
pub fn mark<'a, M: 'a>(size: f32) -> Element<'a, M> {
    static HANDLE: LazyLock<svg::Handle> = LazyLock::new(|| svg::Handle::from_memory(SVG));
    svg(HANDLE.clone()).width(size).height(size).into()
}

/// The logo as RGBA pixels with straight alpha.
fn rasterize(size: u32) -> Option<Vec<u8>> {
    let tree = usvg::Tree::from_data(SVG, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let scale = size as f32 / tree.size().width().max(tree.size().height());
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia stores premultiplied colour, window icons expect straight alpha.
    Some(
        pixmap
            .pixels()
            .iter()
            .flat_map(|pixel| {
                let colour = pixel.demultiply();
                [colour.red(), colour.green(), colour.blue(), colour.alpha()]
            })
            .collect(),
    )
}

/// The window icon. Without one the app starts all the same.
pub fn window_icon() -> Option<window::Icon> {
    window::icon::from_rgba(rasterize(ICON_SIZE)?, ICON_SIZE, ICON_SIZE).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_renders_to_an_opaque_centre_and_round_corners() {
        let pixels = rasterize(64).expect("the bundled logo is a valid SVG");
        assert_eq!(pixels.len(), 64 * 64 * 4);
        let alpha = |x: usize, y: usize| pixels[(y * 64 + x) * 4 + 3];
        assert_eq!(alpha(32, 32), 255);
        // The field has rounded corners, so the very corner stays transparent.
        assert_eq!(alpha(0, 0), 0);
    }
}
