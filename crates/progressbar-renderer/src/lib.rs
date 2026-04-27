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

pub struct FrameRenderer {
    config: ProjectConfig,
    timeline: Timeline,
    layout: Layout,
    base_rgba: Vec<u8>,
    static_label_rgba: Option<Vec<u8>>,
    labels: Vec<CachedLabel>,
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl FrameRenderer {
    pub fn new(config: &ProjectConfig, timeline: &Timeline) -> Result<Self, RenderError> {
        let layout = Layout::calculate(config, timeline)
            .map_err(|error| RenderError::Layout(error.to_string()))?;
        let mut renderer = Self {
            config: config.clone(),
            timeline: timeline.clone(),
            layout,
            base_rgba: Vec::new(),
            static_label_rgba: None,
            labels: Vec::new(),
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
        };
        renderer.base_rgba = renderer.render_base_layer()?;
        renderer.labels = renderer.build_cached_labels();
        renderer.static_label_rgba = renderer.render_static_label_layer()?;
        Ok(renderer)
    }

    pub fn render_frame(&mut self, timestamp_ms: TimeMs) -> Result<RenderedFrame, RenderError> {
        let mut pixmap = Pixmap::new(self.config.render.width, self.config.render.height)
            .ok_or(RenderError::Allocation)?;
        pixmap.data_mut().copy_from_slice(&self.base_rgba);

        if self.config.playback_progress.enabled {
            draw_playback_progress(
                &mut pixmap,
                &self.config,
                &self.layout,
                self.timeline.duration_ms(),
                timestamp_ms,
            )?;
        }
        if let Some(static_label_rgba) = &self.static_label_rgba {
            blend_rgba_layer(&mut pixmap, static_label_rgba);
        }
        self.draw_dynamic_labels(&mut pixmap, timestamp_ms)?;

        Ok(RenderedFrame {
            width: self.config.render.width,
            height: self.config.render.height,
            rgba: pixmap.take(),
        })
    }

    fn render_base_layer(&self) -> Result<Vec<u8>, RenderError> {
        let mut pixmap = Pixmap::new(self.config.render.width, self.config.render.height)
            .ok_or(RenderError::Allocation)?;

        fill_rect(&mut pixmap, self.layout.bar, &self.config.bar.track_color)?;
        for segment in &self.layout.segments {
            fill_rect(&mut pixmap, segment.rect, &self.config.bar.fill_color)?;
            let divider = progressbar_core::Rect {
                x: segment.rect.x.round(),
                y: segment.rect.y,
                width: 1.0,
                height: segment.rect.height,
            };
            fill_rect(&mut pixmap, divider, &self.config.bar.divider_color)?;
        }

        Ok(pixmap.take())
    }

    fn build_cached_labels(&self) -> Vec<CachedLabel> {
        self.layout
            .segments
            .iter()
            .map(|segment_layout| {
                let segment = &self.timeline.segments[segment_layout.segment_index];
                let plan = plan_label_render(
                    &self.config,
                    segment_layout.rect,
                    &segment.label,
                    segment.start_ms,
                    segment.start_ms,
                    segment.end_ms,
                );
                let mode = match plan.mode {
                    LabelRenderMode::Normal => CachedLabelMode::Normal,
                    LabelRenderMode::Rotate => CachedLabelMode::Rotate,
                    LabelRenderMode::Scroll { .. } => CachedLabelMode::Scroll,
                };
                let measured_width = estimate_text_width(&plan.text, plan.font_size);
                CachedLabel {
                    segment_index: segment_layout.segment_index,
                    rect: segment_layout.rect,
                    text: plan.text,
                    font_size: plan.font_size,
                    mode,
                    start_ms: segment.start_ms,
                    end_ms: segment.end_ms,
                    measured_width,
                }
            })
            .collect()
    }

    fn render_static_label_layer(&mut self) -> Result<Option<Vec<u8>>, RenderError> {
        if self.config.text.display_mode != progressbar_schema::TextDisplayMode::AllSegments {
            return Ok(None);
        }

        let labels: Vec<CachedLabel> = self
            .labels
            .iter()
            .filter(|label| label.mode != CachedLabelMode::Scroll)
            .cloned()
            .collect();
        if labels.is_empty() {
            return Ok(None);
        }

        let mut pixmap = Pixmap::new(self.config.render.width, self.config.render.height)
            .ok_or(RenderError::Allocation)?;
        for label in &labels {
            let plan = label.render_plan_at(label.start_ms);
            draw_text_pixels(
                &mut pixmap,
                &self.config,
                label.rect,
                &plan,
                &mut self.font_system,
                &mut self.swash_cache,
            )?;
        }

        Ok(Some(pixmap.take()))
    }

    fn draw_dynamic_labels(
        &mut self,
        pixmap: &mut Pixmap,
        timestamp_ms: TimeMs,
    ) -> Result<(), RenderError> {
        let active = self.timeline.active_segment_index(timestamp_ms);
        for label in &self.labels {
            if self.config.text.display_mode == progressbar_schema::TextDisplayMode::AllSegments
                && label.mode != CachedLabelMode::Scroll
            {
                continue;
            }

            let should_draw = match self.config.text.display_mode {
                progressbar_schema::TextDisplayMode::AllSegments => true,
                progressbar_schema::TextDisplayMode::ActiveOnly => {
                    Some(label.segment_index) == active
                }
                progressbar_schema::TextDisplayMode::PastCurrent => active
                    .map(|active_index| label.segment_index <= active_index)
                    .unwrap_or(false),
                progressbar_schema::TextDisplayMode::None => false,
            };
            if should_draw {
                let plan = label.render_plan_at(timestamp_ms);
                draw_text_pixels(
                    pixmap,
                    &self.config,
                    label.rect,
                    &plan,
                    &mut self.font_system,
                    &mut self.swash_cache,
                )?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct CachedLabel {
    segment_index: usize,
    rect: progressbar_core::Rect,
    text: String,
    font_size: u32,
    mode: CachedLabelMode,
    start_ms: TimeMs,
    end_ms: TimeMs,
    measured_width: f32,
}

impl CachedLabel {
    fn render_plan_at(&self, timestamp_ms: TimeMs) -> LabelRenderPlan {
        let mode = match self.mode {
            CachedLabelMode::Normal => LabelRenderMode::Normal,
            CachedLabelMode::Rotate => LabelRenderMode::Rotate,
            CachedLabelMode::Scroll => LabelRenderMode::Scroll {
                offset_px: self.scroll_offset(timestamp_ms),
            },
        };
        LabelRenderPlan {
            text: self.text.clone(),
            font_size: self.font_size,
            mode,
        }
    }

    fn scroll_offset(&self, timestamp_ms: TimeMs) -> f32 {
        if self.measured_width <= self.rect.width || self.end_ms <= self.start_ms {
            return 0.0;
        }
        let elapsed = timestamp_ms
            .saturating_sub(self.start_ms)
            .min(self.end_ms - self.start_ms);
        let ratio = elapsed as f32 / (self.end_ms - self.start_ms) as f32;
        -(self.measured_width - self.rect.width) * ratio
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CachedLabelMode {
    Normal,
    Rotate,
    Scroll,
}

pub fn render_frame(
    config: &ProjectConfig,
    timeline: &Timeline,
    timestamp_ms: TimeMs,
) -> Result<RenderedFrame, RenderError> {
    FrameRenderer::new(config, timeline)?.render_frame(timestamp_ms)
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

fn draw_text_pixels(
    pixmap: &mut Pixmap,
    config: &ProjectConfig,
    rect: progressbar_core::Rect,
    plan: &LabelRenderPlan,
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
) -> Result<(), RenderError> {
    if plan.text.is_empty() {
        return Ok(());
    }

    let font_size = plan.font_size as f32;
    let line_height = (font_size * 1.2).max(font_size + 2.0);
    let metrics = Metrics::new(font_size, line_height);
    let mut buffer = Buffer::new(font_system, metrics);
    let attrs = Attrs::new().family(Family::Name(&config.text.font_family));
    let text_width = estimate_text_width(&plan.text, plan.font_size);
    let buffer_width = text_width.max(rect.width).ceil() + 8.0;
    let buffer_height = line_height.ceil() + 8.0;

    {
        let mut borrowed = buffer.borrow_with(font_system);
        borrowed.set_size(Some(buffer_width.max(1.0)), Some(buffer_height.max(1.0)));
        borrowed.set_text(&plan.text, &attrs, Shaping::Advanced, None);
    }

    let text_rgba = parse_color_components(&config.text.color)?;
    let text_color = TextColor::rgba(text_rgba[0], text_rgba[1], text_rgba[2], text_rgba[3]);
    let clip_left = rect.x.max(0.0) as i32;
    let clip_top = rect.y.max(0.0) as i32;
    let clip_right = (rect.x + rect.width).min(pixmap.width() as f32) as i32;
    let clip_bottom = (rect.y + rect.height).min(pixmap.height() as f32) as i32;
    let baseline_y = rect.y + (rect.height - font_size) / 2.0;
    let base_x = match plan.mode {
        LabelRenderMode::Normal | LabelRenderMode::Rotate => rect.x + 4.0,
        LabelRenderMode::Scroll { offset_px } => rect.x + 4.0 + offset_px,
    };
    let rotated_x = rect.x + (rect.width - line_height) / 2.0;
    let rotated_y = rect.y + 4.0;

    let mut borrowed = buffer.borrow_with(font_system);
    borrowed.draw(swash_cache, text_color, |x, y, width, height, color| {
        let [r, g, b, a] = color.as_rgba();
        for dy in 0..height as i32 {
            for dx in 0..width as i32 {
                let (px, py) = match plan.mode {
                    LabelRenderMode::Normal | LabelRenderMode::Scroll { .. } => (
                        base_x.round() as i32 + x + dx,
                        baseline_y.round() as i32 + y + dy,
                    ),
                    LabelRenderMode::Rotate => {
                        let source_x = (x + dx) as f32;
                        let source_y = (y + dy) as f32 + font_size;
                        (
                            (rotated_x + line_height - source_y).round() as i32,
                            (rotated_y + source_x).round() as i32,
                        )
                    }
                };
                if px >= clip_left && px < clip_right && py >= clip_top && py < clip_bottom {
                    blend_pixel(pixmap, px as u32, py as u32, [r, g, b, a]);
                }
            }
        }
    });

    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
struct LabelRenderPlan {
    text: String,
    font_size: u32,
    mode: LabelRenderMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum LabelRenderMode {
    Normal,
    Rotate,
    Scroll { offset_px: f32 },
}

fn estimate_text_width(text: &str, font_size: u32) -> f32 {
    text.chars()
        .map(|ch| if ch.is_ascii() { 0.58 } else { 1.0 })
        .sum::<f32>()
        * font_size as f32
}

fn can_rotate_label(rect: progressbar_core::Rect, min_font_size: u32) -> bool {
    rect.height >= min_font_size as f32 * 2.5
}

fn plan_label_render(
    config: &ProjectConfig,
    rect: progressbar_core::Rect,
    label: &str,
    timestamp_ms: TimeMs,
    segment_start_ms: TimeMs,
    segment_end_ms: TimeMs,
) -> LabelRenderPlan {
    let text_width = estimate_text_width(label, config.text.font_size);
    let decision = progressbar_core::choose_text_strategy(progressbar_core::TextStrategyInput {
        overflow: config.text.overflow.clone(),
        text_width_px: text_width,
        rect_width_px: rect.width.max(1.0),
        font_size: config.text.font_size,
        min_font_size: config.text.min_font_size,
        can_rotate: can_rotate_label(rect, config.text.min_font_size),
    });

    match decision {
        progressbar_core::TextStrategyDecision::Normal { font_size }
        | progressbar_core::TextStrategyDecision::Shrink { font_size } => LabelRenderPlan {
            text: label.to_string(),
            font_size,
            mode: LabelRenderMode::Normal,
        },
        progressbar_core::TextStrategyDecision::Ellipsis { font_size } => LabelRenderPlan {
            text: ellipsize_to_width(label, font_size, rect.width.max(1.0)),
            font_size,
            mode: LabelRenderMode::Normal,
        },
        progressbar_core::TextStrategyDecision::Rotate { font_size } => LabelRenderPlan {
            text: label.to_string(),
            font_size,
            mode: LabelRenderMode::Rotate,
        },
        progressbar_core::TextStrategyDecision::Scroll { font_size } => {
            let segment = progressbar_core::Segment {
                start_ms: segment_start_ms,
                end_ms: segment_end_ms,
                label: label.to_string(),
            };
            let measured = estimate_text_width(label, font_size);
            LabelRenderPlan {
                text: label.to_string(),
                font_size,
                mode: LabelRenderMode::Scroll {
                    offset_px: progressbar_core::scroll_offset_px(
                        timestamp_ms,
                        &segment,
                        measured,
                        rect.width,
                    ),
                },
            }
        }
    }
}

fn ellipsize_to_width(label: &str, font_size: u32, max_width: f32) -> String {
    if estimate_text_width(label, font_size) <= max_width {
        return label.to_string();
    }

    let ellipsis = "...";
    let mut output = String::new();
    for ch in label.chars() {
        let candidate = format!("{output}{ch}{ellipsis}");
        if estimate_text_width(&candidate, font_size) > max_width {
            break;
        }
        output.push(ch);
    }

    if output.is_empty() {
        ellipsis.to_string()
    } else {
        format!("{output}{ellipsis}")
    }
}

fn blend_rgba_layer(pixmap: &mut Pixmap, layer: &[u8]) {
    let width = pixmap.width();
    for (index, pixel) in layer.chunks_exact(4).enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let x = index as u32 % width;
        let y = index as u32 / width;
        blend_pixel(pixmap, x, y, [pixel[0], pixel[1], pixel[2], pixel[3]]);
    }
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
    fn ellipsis_plan_shortens_text_to_fit_rect() {
        let mut config = ProjectConfig::default();
        config.text.overflow = progressbar_schema::OverflowMode::Ellipsis;
        config.text.font_size = 24;
        config.text.min_font_size = 14;
        let rect = progressbar_core::Rect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 40.0,
        };
        let plan = plan_label_render(&config, rect, "这是一个非常非常长的标题", 0, 0, 2_000);
        assert!(matches!(plan.mode, LabelRenderMode::Normal));
        assert!(plan.text.ends_with("..."));
        assert!(estimate_text_width(&plan.text, plan.font_size) <= rect.width);
    }

    #[test]
    fn auto_scroll_plan_uses_min_font_size_when_rotation_is_not_available() {
        let mut config = ProjectConfig::default();
        config.text.overflow = progressbar_schema::OverflowMode::Auto;
        config.text.font_size = 28;
        config.text.min_font_size = 16;
        let rect = progressbar_core::Rect {
            x: 0.0,
            y: 0.0,
            width: 70.0,
            height: 12.0,
        };
        let plan = plan_label_render(&config, rect, "这是一个非常非常长的标题", 1_000, 0, 2_000);
        assert_eq!(plan.font_size, 16);
        assert!(matches!(plan.mode, LabelRenderMode::Scroll { .. }));
    }

    #[test]
    fn auto_rotate_plan_uses_min_font_size_for_narrow_tall_cells() {
        let mut config = ProjectConfig::default();
        config.text.overflow = progressbar_schema::OverflowMode::Auto;
        config.text.font_size = 28;
        config.text.min_font_size = 16;
        let rect = progressbar_core::Rect {
            x: 0.0,
            y: 0.0,
            width: 70.0,
            height: 64.0,
        };
        let plan = plan_label_render(&config, rect, "这是一个非常非常长的标题", 1_000, 0, 2_000);
        assert_eq!(plan.font_size, 16);
        assert!(matches!(plan.mode, LabelRenderMode::Rotate));
    }

    #[test]
    fn scroll_mode_changes_visible_pixels_over_segment_time() {
        let mut config = ProjectConfig::default();
        config.render.width = 240;
        config.render.height = 120;
        config.bar.margin_x = 20;
        config.bar.margin_bottom = 16;
        config.bar.height = 32;
        config.playback_progress.enabled = false;
        config.text.overflow = progressbar_schema::OverflowMode::Scroll;
        config.text.font_size = 22;
        let timeline = Timeline::parse("2 | very very very long label").unwrap();
        let start = render_frame(&config, &timeline, 0).unwrap();
        let later = render_frame(&config, &timeline, 1_800).unwrap();
        assert_ne!(sample_bar_region(&start), sample_bar_region(&later));
    }

    #[test]
    fn rotate_mode_renders_text_across_vertical_segment_space() {
        let mut config = ProjectConfig::default();
        config.render.width = 220;
        config.render.height = 140;
        config.bar.margin_x = 20;
        config.bar.margin_bottom = 16;
        config.bar.height = 72;
        config.playback_progress.enabled = false;
        config.text.overflow = progressbar_schema::OverflowMode::Rotate;
        config.text.font_size = 20;
        let timeline = Timeline::parse("2 | rotate label").unwrap();
        let frame = render_frame(&config, &timeline, 500).unwrap();
        assert!(text_pixel_row_span(&frame) > 35);
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

    #[test]
    fn frame_renderer_matches_one_shot_render_frame() {
        let mut config = ProjectConfig::default();
        config.render.width = 320;
        config.render.height = 180;
        config.bar.margin_x = 20;
        config.bar.margin_bottom = 16;
        config.bar.height = 36;
        let timeline = Timeline::parse("1 | A\n2 | very very long label").unwrap();
        let mut renderer = FrameRenderer::new(&config, &timeline).unwrap();
        let cached = renderer.render_frame(1_500).unwrap();
        let one_shot = render_frame(&config, &timeline, 1_500).unwrap();
        assert_eq!(cached.rgba, one_shot.rgba);
    }

    #[test]
    fn frame_renderer_keeps_static_frames_identical() {
        let mut config = ProjectConfig::default();
        config.render.width = 320;
        config.render.height = 180;
        config.bar.margin_x = 20;
        config.bar.margin_bottom = 16;
        config.bar.height = 36;
        config.playback_progress.enabled = false;
        config.text.overflow = progressbar_schema::OverflowMode::Ellipsis;
        let timeline = Timeline::parse("1 | very very long label\n2 | B").unwrap();
        let mut renderer = FrameRenderer::new(&config, &timeline).unwrap();
        let first = renderer.render_frame(100).unwrap();
        let second = renderer.render_frame(1_900).unwrap();
        assert_eq!(first.rgba, second.rgba);
    }

    fn sample_bar_region(frame: &RenderedFrame) -> Vec<[u8; 4]> {
        (60..90)
            .flat_map(|y| (30..210).step_by(3).map(move |x| frame.pixel_rgba(x, y)))
            .collect()
    }

    fn text_pixel_row_span(frame: &RenderedFrame) -> usize {
        let fill = [77, 163, 255, 255];
        let rows: Vec<u32> = (0..frame.height)
            .filter(|y| {
                (0..frame.width).any(|x| {
                    let pixel = frame.pixel_rgba(x, *y);
                    pixel[3] > 0 && pixel != fill
                })
            })
            .collect();

        match (rows.first(), rows.last()) {
            (Some(first), Some(last)) => (last - first + 1) as usize,
            _ => 0,
        }
    }
}
