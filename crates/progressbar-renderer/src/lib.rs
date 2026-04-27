use cosmic_text::{
    Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics, Shaping, SwashCache,
};
use progressbar_core::{Layout, TimeMs, Timeline};
use progressbar_schema::ProjectConfig;
use std::io::Write;
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
    #[error("png write failed: {0}")]
    Png(String),
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
            x: segment.rect.x.round(),
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
    draw_labels(&mut pixmap, config, timeline, &layout, timestamp_ms)?;

    Ok(RenderedFrame {
        width: config.render.width,
        height: config.render.height,
        rgba: pixmap.take(),
    })
}

pub fn write_png<W: Write>(frame: &RenderedFrame, writer: W) -> Result<(), RenderError> {
    let mut encoder = png::Encoder::new(writer, frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png_writer = encoder
        .write_header()
        .map_err(|error| RenderError::Png(error.to_string()))?;
    png_writer
        .write_image_data(&frame.rgba)
        .map_err(|error| RenderError::Png(error.to_string()))?;
    Ok(())
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

fn draw_labels(
    pixmap: &mut Pixmap,
    config: &ProjectConfig,
    timeline: &Timeline,
    layout: &Layout,
    timestamp_ms: TimeMs,
) -> Result<(), RenderError> {
    let active = timeline.active_segment_index(timestamp_ms);
    for segment_layout in &layout.segments {
        let segment = &timeline.segments[segment_layout.segment_index];
        let should_draw = match config.text.display_mode {
            progressbar_schema::TextDisplayMode::AllSegments => true,
            progressbar_schema::TextDisplayMode::ActiveOnly => {
                Some(segment_layout.segment_index) == active
            }
            progressbar_schema::TextDisplayMode::PastCurrent => active
                .map(|active_index| segment_layout.segment_index <= active_index)
                .unwrap_or(false),
            progressbar_schema::TextDisplayMode::None => false,
        };
        if should_draw {
            draw_label_text(pixmap, config, segment_layout.rect, &segment.label)?;
        }
    }
    Ok(())
}

fn draw_label_text(
    pixmap: &mut Pixmap,
    config: &ProjectConfig,
    rect: progressbar_core::Rect,
    label: &str,
) -> Result<(), RenderError> {
    if rect.width <= 1.0 || rect.height <= 1.0 || label.is_empty() {
        return Ok(());
    }

    let mut font_system = FontSystem::new();
    let mut swash_cache = SwashCache::new();
    let font_size = config.text.font_size as f32;
    let metrics = Metrics::new(font_size, (font_size * 1.2).max(font_size + 2.0));
    let mut buffer = Buffer::new(&mut font_system, metrics);
    let attrs = Attrs::new().family(Family::Name(&config.text.font_family));

    {
        let mut borrowed = buffer.borrow_with(&mut font_system);
        borrowed.set_size(Some(rect.width.max(1.0)), Some(rect.height.max(1.0)));
        borrowed.set_text(label, &attrs, Shaping::Advanced, None);
    }

    let text_rgba = parse_color_components(&config.text.color)?;
    let text_color = TextColor::rgba(text_rgba[0], text_rgba[1], text_rgba[2], text_rgba[3]);
    let clip_left = rect.x.max(0.0) as i32;
    let clip_top = rect.y.max(0.0) as i32;
    let clip_right = (rect.x + rect.width).min(pixmap.width() as f32) as i32;
    let clip_bottom = (rect.y + rect.height).min(pixmap.height() as f32) as i32;
    let baseline_y = rect.y + (rect.height - font_size) / 2.0;

    let mut borrowed = buffer.borrow_with(&mut font_system);
    borrowed.draw(
        &mut swash_cache,
        text_color,
        |x, y, width, height, color| {
            let [r, g, b, a] = color.as_rgba();
            for dy in 0..height as i32 {
                for dx in 0..width as i32 {
                    let px = rect.x as i32 + x + dx;
                    let py = baseline_y as i32 + y + dy;
                    if px >= clip_left && px < clip_right && py >= clip_top && py < clip_bottom {
                        blend_pixel(pixmap, px as u32, py as u32, [r, g, b, a]);
                    }
                }
            }
        },
    );

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
    let [r, g, b, a] = parse_color_components(value)?;
    Ok(Color::from_rgba8(r, g, b, a))
}

fn parse_color_components(value: &str) -> Result<[u8; 4], RenderError> {
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
    Ok([r, g, b, a])
}

fn blend_pixel(pixmap: &mut Pixmap, x: u32, y: u32, src: [u8; 4]) {
    let width = pixmap.width();
    let index = ((y * width + x) * 4) as usize;
    let data = pixmap.data_mut();
    let src_a = src[3] as f32 / 255.0;
    let dst_a = data[index + 3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= f32::EPSILON {
        return;
    }
    for channel in 0..3 {
        let src_c = src[channel] as f32 / 255.0;
        let dst_c = data[index + channel] as f32 / 255.0;
        data[index + channel] =
            (((src_c * src_a + dst_c * dst_a * (1.0 - src_a)) / out_a) * 255.0) as u8;
    }
    data[index + 3] = (out_a * 255.0) as u8;
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

    #[test]
    fn renders_fractional_duration_dividers_without_panic() {
        let mut config = ProjectConfig::default();
        config.render.width = 640;
        config.render.height = 360;
        config.bar.margin_x = 40;
        config.bar.margin_bottom = 24;
        config.bar.height = 48;
        config.playback_progress.height = 5;
        let timeline = Timeline::parse("2 | A\n4 | B\n6 | C").unwrap();
        let frame = render_frame(&config, &timeline, 1_000).unwrap();
        assert_eq!(frame.width, 640);
    }

    #[test]
    fn renders_label_pixels_inside_segment_area() {
        let mut config = ProjectConfig::default();
        config.render.width = 640;
        config.render.height = 360;
        config.bar.margin_x = 40;
        config.bar.margin_bottom = 30;
        config.bar.height = 60;
        config.playback_progress.enabled = false;
        config.text.font_size = 24;
        config.text.min_font_size = 16;
        let timeline = Timeline::parse("2 | 开场").unwrap();
        let frame = render_frame(&config, &timeline, 500).unwrap();
        let bar_y = 360 - 30 - 60;
        let fill = [77, 163, 255, 255];
        let has_text_alpha = (bar_y..bar_y + 60).any(|y| {
            (80..600).any(|x| {
                let pixel = frame.pixel_rgba(x, y);
                pixel[3] > 0 && pixel != fill
            })
        });
        assert!(has_text_alpha);
    }
}
