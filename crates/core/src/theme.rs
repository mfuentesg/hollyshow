//! Visual themes (PLAN §4). Serialised as JSON in `themes.json`.

use crate::ids::{MediaId, ThemeId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Background {
    Color {
        color: String,
    },
    Gradient {
        from: String,
        to: String,
        angle_deg: f32,
    },
    Image {
        media_id: MediaId,
        blur_px: f32,
        ken_burns: bool,
    },
    Video {
        media_id: MediaId,
        loop_playback: bool,
        muted: bool,
    },
    /// Fully transparent: OBS / lower-third outputs.
    Transparent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FontSpec {
    pub family: String,
    pub weight: u16,
    pub min_px: f32,
    pub max_px: f32,
    /// When true every slide of an item shares one size (avoids size jumps).
    pub uniform_size: bool,
    pub italic: bool,
    pub uppercase: bool,
    pub line_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TransitionKind {
    Cut,
    Fade,
    Crossfade,
    Slide,
    Zoom,
    Blur,
    /// WebGL shader transition (level 2).
    Shader,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TransitionSpec {
    pub kind: TransitionKind,
    pub duration_ms: u32,
    /// Reveal text line by line (PLAN §2.2).
    pub line_by_line: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Theme {
    pub id: ThemeId,
    pub name: String,
    pub background: Background,
    pub font: FontSpec,
    pub text_color: String,
    pub shadow: Option<String>,
    pub align: TextAlign,
    /// Safe area as percentage of the output.
    pub margins: Margins,
    pub transition: TransitionSpec,
    pub translation_scale: f32,
    pub show_bible_version_label: bool,
    pub show_copyright: bool,
    pub max_lines_per_slide: u8,
}

impl Theme {
    /// Built-in default theme, derived from the "Cabina nocturna" design tokens.
    pub fn default_theme() -> Self {
        Self {
            id: ThemeId::new("default"),
            name: "Predeterminado".into(),
            background: Background::Color {
                color: "#0F1115".into(),
            },
            font: FontSpec {
                family: "Archivo".into(),
                weight: 800,
                min_px: 36.0,
                max_px: 120.0,
                uniform_size: true,
                italic: false,
                uppercase: false,
                line_height: 1.2,
            },
            text_color: "#FFFFFF".into(),
            shadow: Some("0 2px 8px rgba(0,0,0,.6)".into()),
            align: TextAlign::Center,
            margins: Margins {
                top: 6.0,
                right: 8.0,
                bottom: 6.0,
                left: 8.0,
            },
            transition: TransitionSpec {
                kind: TransitionKind::Crossfade,
                duration_ms: 250,
                line_by_line: false,
            },
            translation_scale: 0.6,
            show_bible_version_label: true,
            show_copyright: true,
            max_lines_per_slide: 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_json_round_trip() {
        let t = Theme::default_theme();
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<Theme>(&json).unwrap(), t);
        assert!(json.contains("\"textColor\""));
    }
}
