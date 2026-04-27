use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Background {
    Transparent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BarPosition {
    Bottom,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TextDisplayMode {
    AllSegments,
    ActiveOnly,
    PastCurrent,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OverflowMode {
    Shrink,
    Ellipsis,
    Rotate,
    Scroll,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    PngSequence,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(default)]
pub struct RenderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub background: Background,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 60,
            background: Background::Transparent,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(default)]
pub struct BarConfig {
    pub position: BarPosition,
    pub height: u32,
    pub margin_x: u32,
    pub margin_bottom: u32,
    pub corner_radius: u32,
    pub track_color: String,
    pub fill_color: String,
    pub divider_color: String,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            position: BarPosition::Bottom,
            height: 72,
            margin_x: 80,
            margin_bottom: 36,
            corner_radius: 8,
            track_color: "#FFFFFF33".to_string(),
            fill_color: "#4DA3FF".to_string(),
            divider_color: "#FFFFFFAA".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(default)]
pub struct PlaybackProgressConfig {
    pub enabled: bool,
    pub height: u32,
    pub offset_y: i32,
    pub color: String,
    pub track_color: String,
    pub thumb_enabled: bool,
    pub thumb_radius: u32,
    pub thumb_color: String,
}

impl Default for PlaybackProgressConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            height: 6,
            offset_y: 0,
            color: "#FFFFFF".to_string(),
            track_color: "#00000033".to_string(),
            thumb_enabled: true,
            thumb_radius: 7,
            thumb_color: "#FFFFFF".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(default)]
pub struct TextConfig {
    pub font_family: String,
    pub font_size: u32,
    pub min_font_size: u32,
    pub color: String,
    pub display_mode: TextDisplayMode,
    pub overflow: OverflowMode,
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            font_family: "Microsoft YaHei".to_string(),
            font_size: 28,
            min_font_size: 18,
            color: "#FFFFFF".to_string(),
            display_mode: TextDisplayMode::AllSegments,
            overflow: OverflowMode::Auto,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(default)]
pub struct OutputConfig {
    pub format: OutputFormat,
    pub path: PathBuf,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: OutputFormat::PngSequence,
            path: PathBuf::from("out/progress"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ProjectConfig {
    pub render: RenderConfig,
    pub bar: BarConfig,
    pub playback_progress: PlaybackProgressConfig,
    pub text: TextConfig,
    pub output: OutputConfig,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("failed to parse TOML config: {0}")]
    Parse(String),
    #[error("render dimensions and fps must be greater than zero")]
    InvalidRenderDimensions,
    #[error("bar dimensions or margins do not fit inside the render frame")]
    InvalidBarGeometry,
    #[error("text font_size and min_font_size must be greater than zero, and min_font_size must not exceed font_size")]
    InvalidTextSize { font_size: u32, min_font_size: u32 },
    #[error("color value `{value}` is not a supported #RRGGBB or #RRGGBBAA color")]
    InvalidColor { value: String },
}

impl ProjectConfig {
    pub fn from_toml_str(input: &str) -> Result<Self, ConfigError> {
        let config: Self =
            toml::from_str(input).map_err(|error| ConfigError::Parse(error.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.render.width == 0 || self.render.height == 0 || self.render.fps == 0 {
            return Err(ConfigError::InvalidRenderDimensions);
        }
        if self.bar.height == 0
            || self.bar.margin_x.saturating_mul(2) >= self.render.width
            || self.bar.height + self.bar.margin_bottom > self.render.height
        {
            return Err(ConfigError::InvalidBarGeometry);
        }
        if self.text.font_size == 0
            || self.text.min_font_size == 0
            || self.text.min_font_size > self.text.font_size
        {
            return Err(ConfigError::InvalidTextSize {
                font_size: self.text.font_size,
                min_font_size: self.text.min_font_size,
            });
        }

        validate_color(&self.bar.track_color)?;
        validate_color(&self.bar.fill_color)?;
        validate_color(&self.bar.divider_color)?;
        validate_color(&self.playback_progress.color)?;
        validate_color(&self.playback_progress.track_color)?;
        validate_color(&self.playback_progress.thumb_color)?;
        validate_color(&self.text.color)?;
        Ok(())
    }
}

pub fn validate_color(value: &str) -> Result<(), ConfigError> {
    let hex = value
        .strip_prefix('#')
        .ok_or_else(|| ConfigError::InvalidColor {
            value: value.to_string(),
        })?;
    let valid_len = hex.len() == 6 || hex.len() == 8;
    let valid_digits = hex.chars().all(|ch| ch.is_ascii_hexdigit());
    if valid_len && valid_digits {
        Ok(())
    } else {
        Err(ConfigError::InvalidColor {
            value: value.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_transparent_overlay_with_auto_text() {
        let config = ProjectConfig::default();
        assert_eq!(config.render.width, 1920);
        assert_eq!(config.render.height, 1080);
        assert_eq!(config.render.fps, 60);
        assert_eq!(config.render.background, Background::Transparent);
        assert_eq!(config.text.overflow, OverflowMode::Auto);
        assert_eq!(config.text.min_font_size, 18);
        assert!(config.playback_progress.enabled);
    }

    #[test]
    fn validates_auto_overflow_requires_min_font_size() {
        let mut config = ProjectConfig::default();
        config.text.overflow = OverflowMode::Auto;
        config.text.min_font_size = 0;
        let error = config.validate().unwrap_err();
        assert!(matches!(error, ConfigError::InvalidTextSize { .. }));
    }

    #[test]
    fn parses_example_toml() {
        let toml = r##"
[render]
width = 1280
height = 720
fps = 30
background = "transparent"

[bar]
position = "bottom"
height = 64
margin_x = 60
margin_bottom = 24
corner_radius = 6
track_color = "#FFFFFF33"
fill_color = "#4DA3FF"
divider_color = "#FFFFFFAA"

[playback_progress]
enabled = true
height = 6
offset_y = 0
color = "#FFFFFF"
track_color = "#00000033"
thumb_enabled = true
thumb_radius = 7
thumb_color = "#FFFFFF"

[text]
font_family = "Microsoft YaHei"
font_size = 28
min_font_size = 18
color = "#FFFFFF"
display_mode = "all-segments"
overflow = "auto"

[output]
format = "png-sequence"
path = "out/progress"
"##;
        let config = ProjectConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.render.width, 1280);
        assert_eq!(config.text.display_mode, TextDisplayMode::AllSegments);
        assert_eq!(config.output.format, OutputFormat::PngSequence);
    }
}
