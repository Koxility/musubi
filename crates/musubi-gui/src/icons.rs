//! Eigene SVG-Icons im 24er-Raster. Gefärbt wird beim Zeichnen, die Dateien sind weiß.

use std::sync::LazyLock;

use iced::widget::svg::Handle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
    SkipPrevious,
    SkipNext,
    Replay10,
    Forward10,
    Subtitles,
    Audio,
    Volume,
    VolumeLow,
    VolumeOff,
    Fullscreen,
    FullscreenExit,
    ArrowBack,
    Stop,
    Check,
    CheckCircle,
    Star,
}

const ALL: [Icon; 18] = [
    Icon::Play,
    Icon::Pause,
    Icon::SkipPrevious,
    Icon::SkipNext,
    Icon::Replay10,
    Icon::Forward10,
    Icon::Subtitles,
    Icon::Audio,
    Icon::Volume,
    Icon::VolumeLow,
    Icon::VolumeOff,
    Icon::Fullscreen,
    Icon::FullscreenExit,
    Icon::ArrowBack,
    Icon::Stop,
    Icon::Check,
    Icon::CheckCircle,
    Icon::Star,
];

/// Gefüllte Formen.
const FILL: &str = r##"fill="#fff""##;
/// Linien mit runden Enden, wie bei modernen Player-Oberflächen üblich.
const STROKE: &str = r##"fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round""##;
/// Dünnere Linien für Ziffern. Ein Attribut darf nur einmal vorkommen, sonst verwirft resvg das
/// ganze Icon.
const STROKE_THIN: &str = r##"fill="none" stroke="#fff" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round""##;

impl Icon {
    fn body(self) -> String {
        let fill = |d: &str| format!(r#"<path {FILL} d="{d}"/>"#);
        let stroke = |d: &str| format!(r#"<path {STROKE} d="{d}"/>"#);
        // „10“ innerhalb der Kreispfeile, als Linien statt Text, damit keine Schrift nötig ist.
        let ten = format!(
            r#"<path {STROKE_THIN} d="M9.5 10.8l1.2-.8v4.4"/><rect {STROKE_THIN} x="12.8" y="10" width="2.6" height="4.4" rx="1.3"/>"#
        );
        match self {
            Self::Play => fill(
                "M8 5.6v12.8a1.1 1.1 0 0 0 1.7.93l10-6.4a1.1 1.1 0 0 0 0-1.86l-10-6.4A1.1 1.1 0 0 0 8 5.6z",
            ),
            Self::Pause => format!(
                r#"<rect {FILL} x="6" y="4.5" width="4.2" height="15" rx="1.3"/><rect {FILL} x="13.8" y="4.5" width="4.2" height="15" rx="1.3"/>"#
            ),
            Self::SkipNext => format!(
                r#"{}<rect {FILL} x="16.4" y="5" width="2.6" height="14" rx="1.1"/>"#,
                fill(
                    "M5 6.4v11.2a1.1 1.1 0 0 0 1.73.9l8-5.6a1.1 1.1 0 0 0 0-1.8l-8-5.6A1.1 1.1 0 0 0 5 6.4z"
                )
            ),
            Self::SkipPrevious => format!(
                r#"{}<rect {FILL} x="5" y="5" width="2.6" height="14" rx="1.1"/>"#,
                fill(
                    "M19 6.4v11.2a1.1 1.1 0 0 1-1.73.9l-8-5.6a1.1 1.1 0 0 1 0-1.8l8-5.6A1.1 1.1 0 0 1 19 6.4z"
                )
            ),
            Self::Replay10 => format!(
                "{}{}{ten}",
                stroke("M3.8 12a8.2 8.2 0 1 0 2.4-5.8"),
                stroke("M3.6 3.8v4.6h4.6")
            ),
            Self::Forward10 => format!(
                "{}{}{ten}",
                stroke("M20.2 12a8.2 8.2 0 1 1-2.4-5.8"),
                stroke("M20.4 3.8v4.6h-4.6")
            ),
            Self::Subtitles => format!(
                r#"<rect {STROKE} x="2.8" y="5" width="18.4" height="14" rx="2.6"/>{}"#,
                stroke("M6.8 11.2h2.4M11.4 11.2h5.8M6.8 15h6M15.2 15h2")
            ),
            Self::Audio => stroke("M4 10v4M8 7v10M12 4v16M16 8v8M20 10.5v3"),
            Self::Volume => format!(
                "{}{}",
                speaker(),
                stroke("M15.5 9a4.2 4.2 0 0 1 0 6M18.4 6.2a8.2 8.2 0 0 1 0 11.6")
            ),
            Self::VolumeLow => format!("{}{}", speaker(), stroke("M15.5 9a4.2 4.2 0 0 1 0 6")),
            Self::VolumeOff => format!("{}{}", speaker(), stroke("M16 9.5l5 5M21 9.5l-5 5")),
            Self::Fullscreen => stroke(
                "M4 9V5.2C4 4.5 4.5 4 5.2 4H9M15 4h3.8c.7 0 1.2.5 1.2 1.2V9M20 15v3.8c0 .7-.5 1.2-1.2 1.2H15M9 20H5.2c-.7 0-1.2-.5-1.2-1.2V15",
            ),
            Self::FullscreenExit => stroke(
                "M9 4v3.8C9 8.5 8.5 9 7.8 9H4M20 9h-3.8C15.5 9 15 8.5 15 7.8V4M15 20v-3.8c0-.7.5-1.2 1.2-1.2H20M4 15h3.8c.7 0 1.2.5 1.2 1.2V20",
            ),
            Self::ArrowBack => stroke("M19 12H5.5M11.5 18l-6-6 6-6"),
            Self::Stop => format!(r#"<rect {FILL} x="6" y="6" width="12" height="12" rx="2.2"/>"#),
            Self::Check => stroke("M5 12.5l4.5 4.5L19 7.5"),
            Self::CheckCircle => format!(
                r#"<circle {STROKE} cx="12" cy="12" r="9"/>{}"#,
                stroke("M8 12.3l2.8 2.8L16.2 9.6")
            ),
            Self::Star => fill(
                "M12 3.3l2.7 5.5 6 .9-4.35 4.25 1.03 6L12 17.1l-5.38 2.85 1.03-6L3.3 9.7l6-.9z",
            ),
        }
    }

    fn svg(self) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">{}</svg>"#,
            self.body()
        )
    }

    /// Einmal erzeugte Handles, damit Iced die Rasterung zwischenspeichern kann.
    pub fn handle(self) -> Handle {
        static HANDLES: LazyLock<Vec<Handle>> = LazyLock::new(|| {
            ALL.iter()
                .map(|icon| Handle::from_memory(icon.svg().into_bytes()))
                .collect()
        });
        let index = ALL.iter().position(|i| *i == self).unwrap_or(0);
        HANDLES[index].clone()
    }
}

fn speaker() -> String {
    format!(
        r#"<path {FILL} d="M11 5.2 6.6 9H4.2C3.5 9 3 9.5 3 10.2v3.6c0 .7.5 1.2 1.2 1.2h2.4l4.4 3.8c.4.3 1 .1 1-.4V5.6c0-.5-.6-.8-1-.4z"/>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_parses_and_draws_something() {
        for icon in ALL {
            // Derselbe Parser wie beim Zeichnen: Ein fehlerhaftes Icon bliebe sonst unsichtbar.
            let tree = usvg::Tree::from_str(&icon.svg(), &usvg::Options::default())
                .unwrap_or_else(|e| panic!("{icon:?}: {e}"));
            assert!(tree.root().has_children(), "{icon:?} draws nothing");
        }
        let mut unique = ALL.to_vec();
        unique.dedup();
        assert_eq!(unique.len(), ALL.len());
    }
}
