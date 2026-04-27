use progressbar_core::{Layout, TimeMs, Timeline};
use progressbar_schema::ProjectConfig;
use thiserror::Error;
use tiny_skia::{Color, Paint, Pixmap, Rect, Transform};

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("failed to allocate RGBA frame")]
    Allocation,
    #[error("layout failed: {0}")]
    Layout(String),
    #[error("invalid color `{0}`")]
    Color(String),
}

pub struct RenderedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RenderedFrame {
    pub fn pixel_rgba(&self, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[index],
            self.rgba[index + 1],
            self.rgba[index + 2],
            self.rgba[index + 3],
        ]
    }
}

pub fn render_frame(
    config: &ProjectConfig,
    timeline: &Timeline,
    timestamp_ms: TimeMs,
) -> Result<RenderedFrame, RenderError> {
    let layout = Layout::calculate(config, timeline)
        .map_err(|error| RenderError::Layout(error.to_string()))?;
    let mut pixmap =
        Pixmap::new(config.render.width, config.render.height).ok_or(RenderError::Allocation)?;

    fill_rect(&mut pixmap, layout.bar, &config.bar.track_color)?;
    for segment in &layout.segments {
        fill_rect(&mut pixmap, segment.rect, &config.bar.fill_color)?;
        let divider = progressbar_core::Rect {
            x: segment.rect.x,
            y: segment.rect.y,
            width: 1.0,
            height: segment.rect.height,
        };
        fill_rect(&mut pixmap, divider, &config.bar.divider_color)?;
    }

    if config.playback_progress.enabled {
        draw_playback_progress(
            &mut pixmap,
            config,
            &layout,
            timeline.duration_ms(),
            timestamp_ms,
        )?;
    }

    Ok(RenderedFrame {
        width: config.render.width,
        height: config.render.height,
        rgba: pixmap.take(),
    })
}

fn draw_playback_progress(
    pixmap: &mut Pixmap,
    config: &ProjectConfig,
    layout: &Layout,
    duration_ms: TimeMs,
    timestamp_ms: TimeMs,
) -> Result<(), RenderError> {
    if duration_ms == 0 {
        return Ok(());
    }

    let ratio = (timestamp_ms.min(duration_ms) as f32 / duration_ms as f32).clamp(0.0, 1.0);
    let y = layout.bar.y + layout.bar.height / 2.0 - config.playback_progress.height as f32 / 2.0
        + config.playback_progress.offset_y as f32;
    let track = progressbar_core::Rect {
        x: layout.bar.x,
        y,
        width: layout.bar.width,
        height: config.playback_progress.height as f32,
    };
    fill_rect(pixmap, track, &config.playback_progress.track_color)?;

    let elapsed = progressbar_core::Rect {
        x: layout.bar.x,
        y,
        width: layout.bar.width * ratio,
        height: config.playback_progress.height as f32,
    };
    fill_rect(pixmap, elapsed, &config.playback_progress.color)?;

    if config.playback_progress.thumb_enabled {
        draw_circle(
            pixmap,
            layout.bar.x + layout.bar.width * ratio,
            y + config.playback_progress.height as f32 / 2.0,
            config.playback_progress.thumb_radius as f32,
            &config.playback_progress.thumb_color,
        )?;
    }

    Ok(())
}

fn fill_rect(
    pixmap: &mut Pixmap,
    rect: progressbar_core::Rect,
    color: &str,
) -> Result<(), RenderError> {
    let Some(rect) = Rect::from_xywh(rect.x, rect.y, rect.width.max(0.0), rect.height.max(0.0))
    else {
        return Ok(());
    };
    let mut paint = Paint::default();
    paint.set_color(parse_color(color)?);
    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    Ok(())
}

fn draw_circle(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    radius: f32,
    color: &str,
) -> Result<(), RenderError> {
    let Some(path) = tiny_skia::PathBuilder::from_circle(cx, cy, radius) else {
        return Ok(());
    };
    let mut paint = Paint::default();
    paint.set_color(parse_color(color)?);
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    Ok(())
}

fn parse_color(value: &str) -> Result<Color, RenderError> {
    let hex = value
        .strip_prefix('#')
        .ok_or_else(|| RenderError::Color(value.to_string()))?;
    if hex.len() != 6 && hex.len() != 8 {
        return Err(RenderError::Color(value.to_string()));
    }
    let parse = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16).map_err(|_| RenderError::Color(value.to_string()))
    };
    let r = parse(0..2)?;
    let g = parse(2..4)?;
    let b = parse(4..6)?;
    let a = if hex.len() == 8 { parse(6..8)? } else { 255 };
    Ok(Color::from_rgba8(r, g, b, a))
}

#[cfg(test)]
mod tests {
    use super::*;
    use progressbar_core::Timeline;
    use progressbar_schema::ProjectConfig;

    #[test]
    fn renders_transparent_background_pixels() {
        let config = ProjectConfig::default();
        let timeline = Timeline::parse("2 | 开场").unwrap();
        let frame = render_frame(&config, &timeline, 0).unwrap();
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert_eq!(frame.pixel_rgba(0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn playback_overlay_draws_above_bar_when_enabled() {
        let mut config = ProjectConfig::default();
        config.render.width = 320;
        config.render.height = 180;
        config.bar.margin_x = 20;
        config.bar.margin_bottom = 20;
        config.bar.height = 30;
        config.playback_progress.enabled = true;
        config.playback_progress.height = 6;
        config.playback_progress.thumb_enabled = false;
        let timeline = Timeline::parse("2 | A").unwrap();
        let frame = render_frame(&config, &timeline, 1_000).unwrap();
        let bar_y = 180 - 20 - 30;
        let sample = frame.pixel_rgba(160, bar_y + 15);
        assert!(sample[3] > 0);
    }
}
