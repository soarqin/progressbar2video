use thiserror::Error;

pub type TimeMs = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub start_ms: TimeMs,
    pub end_ms: TimeMs,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeline {
    pub segments: Vec<Segment>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SegmentParseError {
    #[error("line {line}: missing separator `|`")]
    MissingSeparator { line: usize },
    #[error("line {line}: missing segment label")]
    MissingLabel { line: usize },
    #[error("line {line}: invalid time `{value}`")]
    InvalidTime { line: usize, value: String },
    #[error(
        "line {line}: end time {end_ms}ms must be greater than previous end time {previous_end_ms}ms"
    )]
    NonIncreasingTime {
        line: usize,
        previous_end_ms: TimeMs,
        end_ms: TimeMs,
    },
    #[error("segment file contains no segments")]
    Empty,
}

impl Timeline {
    pub fn parse(input: &str) -> Result<Self, SegmentParseError> {
        let mut segments = Vec::new();
        let mut previous_end_ms = 0;

        for (index, raw_line) in input.lines().enumerate() {
            let line_number = index + 1;
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let (time_text, label_text) = trimmed
                .split_once('|')
                .ok_or(SegmentParseError::MissingSeparator { line: line_number })?;
            let label_trimmed = label_text.trim();
            if label_trimmed.is_empty() {
                return Err(SegmentParseError::MissingLabel { line: line_number });
            }
            let label = decode_label_escapes(label_trimmed);

            let end_ms =
                parse_time_ms(time_text.trim()).map_err(|_| SegmentParseError::InvalidTime {
                    line: line_number,
                    value: time_text.trim().to_string(),
                })?;
            if end_ms <= previous_end_ms {
                return Err(SegmentParseError::NonIncreasingTime {
                    line: line_number,
                    previous_end_ms,
                    end_ms,
                });
            }

            segments.push(Segment {
                start_ms: previous_end_ms,
                end_ms,
                label,
            });
            previous_end_ms = end_ms;
        }

        if segments.is_empty() {
            return Err(SegmentParseError::Empty);
        }

        Ok(Self { segments })
    }

    pub fn duration_ms(&self) -> TimeMs {
        self.segments
            .last()
            .map(|segment| segment.end_ms)
            .unwrap_or(0)
    }

    pub fn active_segment_index(&self, timestamp_ms: TimeMs) -> Option<usize> {
        if self.segments.is_empty() {
            return None;
        }
        self.segments
            .iter()
            .position(|segment| timestamp_ms >= segment.start_ms && timestamp_ms < segment.end_ms)
            .or_else(|| {
                let last_index = self.segments.len() - 1;
                (timestamp_ms == self.segments[last_index].end_ms).then_some(last_index)
            })
    }
}

/// Decode the small set of escape sequences accepted in segment labels:
///
/// - `\n` becomes a literal `\n` (line feed) used as a hard line break in
///   every renderer mode.
/// - `\\` becomes a single backslash so users can opt out of the `\n` escape
///   when they need a literal `\n` in the rendered text.
///
/// Any other backslash sequence is preserved verbatim so unknown escapes do
/// not silently swallow characters.
fn decode_label_escapes(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => output.push('\n'),
            Some('\\') => output.push('\\'),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }
    output
}

pub fn parse_time_ms(value: &str) -> Result<TimeMs, ()> {
    let parts: Vec<&str> = value.split(':').collect();
    match parts.as_slice() {
        [seconds] => parse_seconds_ms(seconds),
        [minutes, seconds] => {
            let minutes = minutes.parse::<u64>().map_err(|_| ())?;
            Ok(minutes * 60_000 + parse_seconds_ms(seconds)?)
        }
        [hours, minutes, seconds] => {
            let hours = hours.parse::<u64>().map_err(|_| ())?;
            let minutes = minutes.parse::<u64>().map_err(|_| ())?;
            Ok(hours * 3_600_000 + minutes * 60_000 + parse_seconds_ms(seconds)?)
        }
        _ => Err(()),
    }
}

fn parse_seconds_ms(value: &str) -> Result<TimeMs, ()> {
    let (seconds_text, millis_text) = value.split_once('.').unwrap_or((value, "0"));
    let seconds = seconds_text.parse::<u64>().map_err(|_| ())?;
    let millis = match millis_text.len() {
        0 => 0,
        1 => millis_text.parse::<u64>().map_err(|_| ())? * 100,
        2 => millis_text.parse::<u64>().map_err(|_| ())? * 10,
        _ => millis_text[..3].parse::<u64>().map_err(|_| ())?,
    };
    Ok(seconds * 1_000 + millis)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentLayout {
    pub segment_index: usize,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub bar: Rect,
    pub segments: Vec<SegmentLayout>,
}

#[derive(Debug, Error, PartialEq)]
pub enum LayoutError {
    #[error("timeline duration must be greater than zero")]
    EmptyDuration,
}

impl Layout {
    pub fn calculate(
        config: &progressbar_schema::ProjectConfig,
        timeline: &Timeline,
    ) -> Result<Self, LayoutError> {
        let duration = timeline.duration_ms();
        if duration == 0 {
            return Err(LayoutError::EmptyDuration);
        }

        let x = config.bar.margin_x as f32;
        let width = (config.render.width - config.bar.margin_x * 2) as f32;
        let height = config.bar.height as f32;
        let y = (config.render.height - config.bar.margin_bottom - config.bar.height) as f32;
        let bar = Rect {
            x,
            y,
            width,
            height,
        };

        let segments = timeline
            .segments
            .iter()
            .enumerate()
            .map(|(segment_index, segment)| {
                let start_ratio = segment.start_ms as f32 / duration as f32;
                let end_ratio = segment.end_ms as f32 / duration as f32;
                let segment_x = x + width * start_ratio;
                let segment_width = width * (end_ratio - start_ratio);
                SegmentLayout {
                    segment_index,
                    rect: Rect {
                        x: segment_x,
                        y,
                        width: segment_width,
                        height,
                    },
                }
            })
            .collect();

        Ok(Self { bar, segments })
    }
}

pub fn frame_count(duration_ms: TimeMs, fps: u32) -> u64 {
    if duration_ms == 0 || fps == 0 {
        return 0;
    }
    ((duration_ms as u128 * fps as u128 + 999) / 1000) as u64
}

pub fn frame_timestamp_ms(frame_index: u64, fps: u32) -> TimeMs {
    if fps == 0 {
        return 0;
    }
    ((frame_index as u128 * 1000) / fps as u128) as TimeMs
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextStrategyInput<'a> {
    pub overflow: progressbar_schema::OverflowMode,
    /// Full label text, possibly containing `\n` line feeds. Hard newlines are
    /// honored as forced line breaks in every renderer mode.
    pub text: &'a str,
    pub rect_width_px: f32,
    /// Vertical budget the wrapped text may consume before falling back to
    /// other strategies. Pass the segment cell height for bar-bounded text.
    pub rect_height_px: f32,
    pub font_size: u32,
    pub min_font_size: u32,
    /// Extra pixels between stacked text lines, applied whenever a render
    /// produces more than one line (forced `\n`, wrap, etc.).
    pub line_spacing: u32,
    pub can_rotate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStrategyDecision {
    Normal { font_size: u32 },
    Shrink { font_size: u32 },
    Ellipsis { font_size: u32 },
    Rotate { font_size: u32 },
    Scroll { font_size: u32 },
    Wrap { font_size: u32, lines: u32 },
}

pub fn choose_text_strategy(input: TextStrategyInput<'_>) -> TextStrategyDecision {
    use progressbar_schema::OverflowMode;

    let max_line_width = max_logical_line_width(input.text, input.font_size);
    let forced_lines = forced_line_count(input.text);
    let forced_height = wrapped_height_px(forced_lines, input.font_size, input.line_spacing);

    // Text is `Normal` only when it fits both horizontally (no logical line
    // exceeds the cell width) and vertically (the forced multi-line block
    // fits inside the bar). Multi-line forced labels can therefore still
    // demand an overflow strategy when they get too tall.
    if max_line_width <= input.rect_width_px && forced_height <= input.rect_height_px {
        return TextStrategyDecision::Normal {
            font_size: input.font_size,
        };
    }

    match input.overflow {
        OverflowMode::Shrink => TextStrategyDecision::Shrink {
            // The renderer overrides this in build_cached_labels with a
            // timeline-wide uniform value. Returning `min_font_size` here is
            // a safe per-segment default for callers (mostly tests) that
            // invoke `choose_text_strategy` outside of a timeline context.
            font_size: input.min_font_size,
        },
        OverflowMode::ShrinkFit => {
            // Each segment shrinks independently to the largest size that
            // fits. If even `min_font_size` doesn't fit, the renderer clips.
            let fs = shrink_to_fit(&input).unwrap_or(input.min_font_size);
            TextStrategyDecision::Shrink { font_size: fs }
        }
        OverflowMode::Ellipsis => TextStrategyDecision::Ellipsis {
            font_size: input.font_size,
        },
        OverflowMode::Rotate => TextStrategyDecision::Rotate {
            font_size: input.font_size,
        },
        OverflowMode::Scroll => TextStrategyDecision::Scroll {
            font_size: input.font_size,
        },
        OverflowMode::Wrap => try_shrink_and_wrap(&input).unwrap_or_else(|| {
            // Even at min_font_size the wrap is too tall for the bar; the user
            // explicitly chose `wrap`, so render at min_font_size and let the
            // renderer clip to the bar bounds.
            let lines =
                wrap_line_count_at_size(input.text, input.min_font_size, input.rect_width_px);
            TextStrategyDecision::Wrap {
                font_size: input.min_font_size,
                lines,
            }
        }),
        OverflowMode::Auto => {
            // Wrap takes priority over the other strategies in `auto`. Try
            // wrapping at `font_size` first, then shrink-and-wrap down to
            // `min_font_size`. Only if no size fits do we fall back to the
            // pre-existing shrink/ellipsis/rotate/scroll chain.
            if let Some(decision) = try_shrink_and_wrap(&input) {
                return decision;
            }
            // Shrink fallback: find the largest size in [min, font_size] whose
            // forced multi-line block fits both width and height, then return
            // Shrink. If only `min_font_size` works (or nothing does), fall
            // through to ellipsis/rotate/scroll.
            if let Some(shrunk) = shrink_to_fit(&input) {
                if shrunk > input.min_font_size {
                    return TextStrategyDecision::Shrink { font_size: shrunk };
                }
            }
            if max_line_width <= input.rect_width_px * 1.8 {
                return TextStrategyDecision::Ellipsis {
                    font_size: input.min_font_size,
                };
            }
            if input.can_rotate {
                return TextStrategyDecision::Rotate {
                    font_size: input.min_font_size,
                };
            }
            TextStrategyDecision::Scroll {
                font_size: input.min_font_size,
            }
        }
    }
}

/// Search for the largest font size in `[min_font_size, font_size]` for which
/// the wrapped text height fits the bar's vertical budget. Returns
/// `Some(Shrink)` when the text becomes a single line at that size (no actual
/// wrapping needed) or `Some(Wrap)` when it requires multiple lines. Returns
/// `None` when no size in the range fits.
fn try_shrink_and_wrap(input: &TextStrategyInput<'_>) -> Option<TextStrategyDecision> {
    if input.font_size == 0 || input.min_font_size > input.font_size {
        return None;
    }
    let forced_lines = forced_line_count(input.text);
    for fs in (input.min_font_size..=input.font_size).rev() {
        let lines = wrap_line_count_at_size(input.text, fs, input.rect_width_px);
        let height = wrapped_height_px(lines, fs, input.line_spacing);
        if height <= input.rect_height_px {
            // If the wrap collapses to exactly one physical line per logical
            // line we report Shrink so the renderer stays in single-line
            // semantics for non-wrapping segments.
            if lines == forced_lines {
                return Some(TextStrategyDecision::Shrink { font_size: fs });
            }
            return Some(TextStrategyDecision::Wrap {
                font_size: fs,
                lines,
            });
        }
    }
    None
}

/// Largest font size in `[min_font_size, font_size]` whose forced multi-line
/// block (no auto-wrap) fits both `rect_width_px` and `rect_height_px`. Used
/// by the auto fallback chain when wrap could not satisfy the height budget.
fn shrink_to_fit(input: &TextStrategyInput<'_>) -> Option<u32> {
    if input.font_size == 0 || input.min_font_size > input.font_size {
        return None;
    }
    let forced_lines = forced_line_count(input.text);
    for fs in (input.min_font_size..=input.font_size).rev() {
        let scaled = max_logical_line_width(input.text, fs);
        let height = wrapped_height_px(forced_lines, fs, input.line_spacing);
        if scaled <= input.rect_width_px && height <= input.rect_height_px {
            return Some(fs);
        }
    }
    None
}

/// Largest font size in `[min_font_size, font_size]` for which `text` fits the
/// given cell. Used both for per-segment shrink-fit decisions and as the
/// building block of the timeline-wide uniform shrink. When no size in the
/// range fits, returns `min_font_size` so the renderer can clip a too-wide
/// label rather than disappear it.
pub fn shrink_fit_for_cell(
    text: &str,
    rect_width_px: f32,
    rect_height_px: f32,
    font_size: u32,
    min_font_size: u32,
    line_spacing: u32,
) -> u32 {
    if font_size == 0 {
        return 0;
    }
    if min_font_size >= font_size {
        return font_size;
    }
    let forced_lines = forced_line_count(text);
    for fs in (min_font_size..=font_size).rev() {
        let scaled = max_logical_line_width(text, fs);
        let height = wrapped_height_px(forced_lines, fs, line_spacing);
        if scaled <= rect_width_px && height <= rect_height_px {
            return fs;
        }
    }
    min_font_size
}

/// Smallest [`shrink_fit_for_cell`] across `segments`, used by the uniform
/// `Shrink` overflow mode so every segment renders at the same font size and
/// the most-constrained segment still fits.
///
/// `segments` yields `(text, rect_width_px, rect_height_px)` per cell. When
/// the iterator is empty (e.g. no segments) the input `font_size` is returned
/// unchanged.
pub fn uniform_shrink_size_for_segments<'a, I>(
    segments: I,
    font_size: u32,
    min_font_size: u32,
    line_spacing: u32,
) -> u32
where
    I: IntoIterator<Item = (&'a str, f32, f32)>,
{
    let mut overall: Option<u32> = None;
    for (text, rect_width_px, rect_height_px) in segments {
        let fitted = shrink_fit_for_cell(
            text,
            rect_width_px,
            rect_height_px,
            font_size,
            min_font_size,
            line_spacing,
        );
        overall = Some(match overall {
            Some(prev) => prev.min(fitted),
            None => fitted,
        });
    }
    overall.unwrap_or(font_size)
}

/// Total vertical pixels needed for `lines` text lines. The trailing gap after
/// the last line is intentionally excluded so a single-line wrap takes exactly
/// `font_size` pixels.
pub fn wrapped_height_px(lines: u32, font_size: u32, line_spacing: u32) -> f32 {
    if lines == 0 {
        return 0.0;
    }
    lines as f32 * font_size as f32 + lines.saturating_sub(1) as f32 * line_spacing as f32
}

/// Approximate width of `text` at `font_size`. When the text contains hard
/// newlines, the **widest** logical line determines the overall width, so a
/// caller can compare it against `rect_width_px` to decide whether any
/// horizontal overflow occurs.
pub fn estimate_text_width(text: &str, font_size: u32) -> f32 {
    max_logical_line_width(text, font_size)
}

/// Width of the widest logical line (segments separated by `\n`). Returns 0
/// for empty input.
pub fn max_logical_line_width(text: &str, font_size: u32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    text.split('\n')
        .map(|line| {
            line.chars()
                .map(|ch| char_advance(ch, font_size))
                .sum::<f32>()
        })
        .fold(0.0_f32, f32::max)
}

/// Number of logical lines in `text`, treating `\n` as a hard line break. The
/// minimum is 1 so empty input still represents one line.
pub fn forced_line_count(text: &str) -> u32 {
    if text.is_empty() {
        return 1;
    }
    let count = text.split('\n').count() as u32;
    count.max(1)
}

/// Greedy character-aware wrap that honors `\n` as a hard line break and then
/// splits each logical line into pieces no wider than `max_width`. Empty
/// logical lines are preserved as empty strings so the caller can space them.
pub fn wrap_text_lines(text: &str, font_size: u32, max_width: f32) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let max_width = max_width.max(0.0);
    let mut all_lines = Vec::new();
    for logical in text.split('\n') {
        if logical.is_empty() {
            // Preserve the empty logical line so consecutive `\n`s create
            // visible blank rows in the rendered output.
            all_lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_width = 0.0_f32;
        for ch in logical.chars() {
            let advance = char_advance(ch, font_size);
            if !current.is_empty() && current_width + advance > max_width {
                all_lines.push(std::mem::take(&mut current));
                current_width = 0.0;
            }
            current.push(ch);
            current_width += advance;
        }
        if !current.is_empty() {
            all_lines.push(current);
        }
    }
    all_lines
}

fn wrap_line_count_at_size(text: &str, font_size: u32, max_width: f32) -> u32 {
    if text.is_empty() {
        return 1;
    }
    let count = wrap_text_lines(text, font_size, max_width).len() as u32;
    count.max(1)
}

fn char_advance(ch: char, font_size: u32) -> f32 {
    let ratio = if ch.is_ascii() { 0.58 } else { 1.0 };
    ratio * font_size as f32
}

pub fn scroll_offset_px(
    timestamp_ms: TimeMs,
    segment: &Segment,
    text_width_px: f32,
    rect_width_px: f32,
) -> f32 {
    if text_width_px <= rect_width_px || segment.end_ms <= segment.start_ms {
        return 0.0;
    }
    let elapsed = timestamp_ms
        .saturating_sub(segment.start_ms)
        .min(segment.end_ms - segment.start_ms);
    let ratio = elapsed as f32 / (segment.end_ms - segment.start_ms) as f32;
    -(text_width_px - rect_width_px) * ratio
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_segments_from_end_points() {
        let text = r#"
# intro comment
00:00:12.500 | 开场
00:01:05.000 | 背景介绍
80.250 | 核心演示
"#;
        let timeline = Timeline::parse(text).unwrap();
        assert_eq!(timeline.duration_ms(), 80_250);
        assert_eq!(timeline.segments.len(), 3);
        assert_eq!(timeline.segments[0].start_ms, 0);
        assert_eq!(timeline.segments[0].end_ms, 12_500);
        assert_eq!(timeline.segments[1].start_ms, 12_500);
        assert_eq!(timeline.segments[1].end_ms, 65_000);
        assert_eq!(timeline.segments[2].label, "核心演示");
    }

    #[test]
    fn rejects_non_increasing_end_times() {
        let error = Timeline::parse("10 | A\n9 | B").unwrap_err();
        assert!(matches!(
            error,
            SegmentParseError::NonIncreasingTime { line: 2, .. }
        ));
    }

    #[test]
    fn parses_supported_time_formats() {
        assert_eq!(parse_time_ms("12.500").unwrap(), 12_500);
        assert_eq!(parse_time_ms("01:05.250").unwrap(), 65_250);
        assert_eq!(parse_time_ms("01:02:03.004").unwrap(), 3_723_004);
    }

    #[test]
    fn computes_active_segment_at_timestamp() {
        let timeline = Timeline::parse("10 | A\n20 | B").unwrap();
        assert_eq!(timeline.active_segment_index(0), Some(0));
        assert_eq!(timeline.active_segment_index(10_000), Some(1));
        assert_eq!(timeline.active_segment_index(20_000), Some(1));
        assert_eq!(timeline.active_segment_index(20_001), None);
    }

    #[test]
    fn calculates_duration_proportional_layout() {
        let timeline = Timeline::parse("10 | A\n30 | B").unwrap();
        let config = progressbar_schema::ProjectConfig::default();
        let layout = Layout::calculate(&config, &timeline).unwrap();
        assert_eq!(layout.bar.x, 80.0);
        assert_eq!(layout.bar.width, 1760.0);
        assert_eq!(layout.segments.len(), 2);
        assert!((layout.segments[0].rect.width - 586.6667).abs() < 0.01);
        assert!((layout.segments[1].rect.width - 1173.3333).abs() < 0.01);
    }

    #[test]
    fn derives_frame_count_from_final_end_time() {
        let timeline = Timeline::parse("2 | A").unwrap();
        assert_eq!(frame_count(timeline.duration_ms(), 60), 120);
        assert_eq!(frame_count(2_001, 60), 121);
    }

    #[test]
    fn auto_uses_scroll_only_after_min_font_size() {
        // Bar is too short for any wrap height, so wrap fallback unwinds to
        // the existing shrink/ellipsis/rotate/scroll chain.
        let text = "一".repeat(18);
        let decision = choose_text_strategy(TextStrategyInput {
            overflow: progressbar_schema::OverflowMode::Auto,
            text: &text,
            rect_width_px: 100.0,
            rect_height_px: 30.0,
            font_size: 28,
            min_font_size: 18,
            line_spacing: 4,
            can_rotate: false,
        });
        assert_eq!(decision, TextStrategyDecision::Scroll { font_size: 18 });
    }

    #[test]
    fn auto_prefers_rotation_for_narrow_cells_when_allowed() {
        // Wrap height at min_font_size still exceeds the bar height, so the
        // existing rotation fallback is selected.
        let text = "一".repeat(11);
        let decision = choose_text_strategy(TextStrategyInput {
            overflow: progressbar_schema::OverflowMode::Auto,
            text: &text,
            rect_width_px: 80.0,
            rect_height_px: 60.0,
            font_size: 28,
            min_font_size: 18,
            line_spacing: 4,
            can_rotate: true,
        });
        assert_eq!(decision, TextStrategyDecision::Rotate { font_size: 18 });
    }

    #[test]
    fn explicit_scroll_uses_configured_font_size() {
        let text = "一".repeat(11);
        let decision = choose_text_strategy(TextStrategyInput {
            overflow: progressbar_schema::OverflowMode::Scroll,
            text: &text,
            rect_width_px: 80.0,
            rect_height_px: 60.0,
            font_size: 28,
            min_font_size: 18,
            line_spacing: 4,
            can_rotate: true,
        });
        assert_eq!(decision, TextStrategyDecision::Scroll { font_size: 28 });
    }

    #[test]
    fn auto_picks_wrap_when_lines_fit_bar_height() {
        // 200px text in a 100px-wide cell wraps into 2 lines at font_size 20:
        // height = 2*20 + 1*4 = 44, which fits the 60px bar.
        let text = "一".repeat(10);
        let decision = choose_text_strategy(TextStrategyInput {
            overflow: progressbar_schema::OverflowMode::Auto,
            text: &text,
            rect_width_px: 100.0,
            rect_height_px: 60.0,
            font_size: 20,
            min_font_size: 14,
            line_spacing: 4,
            can_rotate: false,
        });
        assert_eq!(
            decision,
            TextStrategyDecision::Wrap {
                font_size: 20,
                lines: 2,
            }
        );
    }

    #[test]
    fn auto_wrap_shrinks_until_height_fits() {
        // At font_size 24: lines = 4, height = 4*24 + 3*4 = 108 > 70 → too tall.
        // Shrink loop scans down; the first size that fits gets returned.
        let text = "一".repeat(15);
        let decision = choose_text_strategy(TextStrategyInput {
            overflow: progressbar_schema::OverflowMode::Auto,
            text: &text,
            rect_width_px: 100.0,
            rect_height_px: 70.0,
            font_size: 24,
            min_font_size: 14,
            line_spacing: 4,
            can_rotate: false,
        });
        match decision {
            TextStrategyDecision::Wrap { font_size, lines } => {
                assert!(
                    (14..=24).contains(&font_size),
                    "font_size {font_size} out of range",
                );
                assert!(lines >= 2, "expected multi-line wrap, got {lines}");
                let height = wrapped_height_px(lines, font_size, 4);
                assert!(
                    height <= 70.0,
                    "wrap height {height} should fit bar height 70",
                );
            }
            other => panic!("expected Wrap decision, got {other:?}"),
        }
    }

    #[test]
    fn explicit_wrap_clips_when_no_size_fits() {
        // Bar is shorter than even a single line at min_font_size; explicit
        // wrap must still return a Wrap decision (the renderer clips).
        let text = "一".repeat(18);
        let decision = choose_text_strategy(TextStrategyInput {
            overflow: progressbar_schema::OverflowMode::Wrap,
            text: &text,
            rect_width_px: 80.0,
            rect_height_px: 10.0,
            font_size: 28,
            min_font_size: 18,
            line_spacing: 4,
            can_rotate: false,
        });
        assert_eq!(
            decision,
            TextStrategyDecision::Wrap {
                font_size: 18,
                lines: 5,
            }
        );
    }

    #[test]
    fn explicit_wrap_with_short_text_returns_normal() {
        // Text already fits; the early-return path is the same regardless of
        // overflow mode.
        let decision = choose_text_strategy(TextStrategyInput {
            overflow: progressbar_schema::OverflowMode::Wrap,
            text: "一一",
            rect_width_px: 100.0,
            rect_height_px: 60.0,
            font_size: 24,
            min_font_size: 14,
            line_spacing: 4,
            can_rotate: false,
        });
        assert_eq!(decision, TextStrategyDecision::Normal { font_size: 24 });
    }

    #[test]
    fn wrap_text_lines_breaks_long_string_under_width() {
        let lines = wrap_text_lines("abcdefghij", 10, 30.0);
        assert!(lines.len() >= 2, "expected wrap to produce >=2 lines");
        for line in &lines {
            // Each individual line stays at or under the configured width
            // (with sub-pixel rounding tolerance).
            assert!(estimate_text_width(line, 10) <= 30.0 + 0.001);
        }
        assert_eq!(lines.concat(), "abcdefghij");
    }

    #[test]
    fn wrap_text_lines_handles_empty_input() {
        assert!(wrap_text_lines("", 12, 100.0).is_empty());
    }

    #[test]
    fn wrapped_height_excludes_trailing_line_gap() {
        assert_eq!(wrapped_height_px(1, 20, 6), 20.0);
        assert_eq!(wrapped_height_px(3, 20, 6), 3.0 * 20.0 + 2.0 * 6.0);
        assert_eq!(wrapped_height_px(0, 20, 6), 0.0);
    }

    #[test]
    fn segment_parser_decodes_newline_escape() {
        let timeline = Timeline::parse(r"5 | line one\nline two").unwrap();
        assert_eq!(timeline.segments[0].label, "line one\nline two");
    }

    #[test]
    fn segment_parser_keeps_double_backslash_as_literal_escape() {
        // `\\n` in the source means literal backslash + 'n', not a newline.
        let timeline = Timeline::parse(r"5 | path\\nfile").unwrap();
        assert_eq!(timeline.segments[0].label, r"path\nfile");
    }

    #[test]
    fn segment_parser_preserves_unknown_escape_sequences() {
        // Unsupported escapes pass through verbatim so users do not lose data.
        let timeline = Timeline::parse(r"5 | tab\there").unwrap();
        assert_eq!(timeline.segments[0].label, r"tab\there");
    }

    #[test]
    fn estimate_text_width_returns_widest_logical_line() {
        // Longest line wins, regardless of total character count.
        let width = estimate_text_width("ab\nabcdef\nabcd", 10);
        let expected = "abcdef".chars().count() as f32 * 0.58 * 10.0;
        assert!((width - expected).abs() < 0.001);
    }

    #[test]
    fn forced_line_count_counts_logical_lines() {
        assert_eq!(forced_line_count(""), 1);
        assert_eq!(forced_line_count("hello"), 1);
        assert_eq!(forced_line_count("hello\nworld"), 2);
        assert_eq!(forced_line_count("a\n\nb"), 3);
    }

    #[test]
    fn wrap_text_lines_honors_hard_newlines_and_then_wraps_each_line() {
        let lines = wrap_text_lines("abcdef\n12345678", 10, 30.0);
        // First logical line (6 chars) wraps into two physical rows; the
        // second logical line (8 chars) wraps into two as well.
        assert_eq!(lines.len(), 4);
        assert_eq!(&lines[0], "abcde");
        assert_eq!(&lines[1], "f");
        assert_eq!(&lines[2], "12345");
        assert_eq!(&lines[3], "678");
    }

    #[test]
    fn wrap_text_lines_preserves_blank_logical_lines() {
        // Two consecutive `\n`s create an empty logical row that should not
        // disappear from the rendered output.
        let lines = wrap_text_lines("a\n\nb", 10, 100.0);
        assert_eq!(lines, vec!["a".to_string(), String::new(), "b".to_string()]);
    }

    #[test]
    fn forced_newline_inflates_normal_height_check() {
        // A 2-line label cannot stay Normal when the bar height is shorter
        // than the forced multi-line block.
        let decision = choose_text_strategy(TextStrategyInput {
            overflow: progressbar_schema::OverflowMode::Auto,
            text: "ab\ncd",
            rect_width_px: 200.0,
            // Two-line forced block needs 2*20 + 1*4 = 44px, this is 30px.
            rect_height_px: 30.0,
            font_size: 20,
            min_font_size: 12,
            line_spacing: 4,
            can_rotate: false,
        });
        // With width to spare, auto should shrink the forced block to fit.
        match decision {
            TextStrategyDecision::Shrink { font_size }
            | TextStrategyDecision::Wrap { font_size, .. } => {
                assert!(font_size <= 20 && font_size >= 12);
                let block = wrapped_height_px(2, font_size, 4);
                assert!(
                    block <= 30.0,
                    "shrunk block {block}px should fit bar height 30",
                );
            }
            other => panic!("expected Shrink/Wrap, got {other:?}"),
        }
    }

    #[test]
    fn shrink_fit_picks_largest_fs_that_fits_segment() {
        // 10 CJK chars at fs=20 → 200px, cell width 120 forces shrink. Largest
        // fitting size: ceil to widest fs where 10*fs ≤ 120 → fs ≤ 12.
        let fs = shrink_fit_for_cell("一".repeat(10).as_str(), 120.0, 80.0, 20, 8, 4);
        assert_eq!(fs, 12);
        // Same text in a wider cell stays at the configured `font_size`.
        let fs = shrink_fit_for_cell("一".repeat(10).as_str(), 220.0, 80.0, 20, 8, 4);
        assert_eq!(fs, 20);
    }

    #[test]
    fn shrink_fit_falls_back_to_min_font_size_when_nothing_fits() {
        // Text far too wide even at `min_font_size`; helper returns the floor
        // so the renderer can clip rather than fail.
        let fs = shrink_fit_for_cell("一".repeat(40).as_str(), 30.0, 80.0, 20, 14, 4);
        assert_eq!(fs, 14);
    }

    #[test]
    fn shrink_fit_decision_uses_largest_fitting_size() {
        let text = "一".repeat(8);
        let decision = choose_text_strategy(TextStrategyInput {
            overflow: progressbar_schema::OverflowMode::ShrinkFit,
            text: &text,
            rect_width_px: 100.0,
            rect_height_px: 60.0,
            font_size: 20,
            min_font_size: 10,
            line_spacing: 4,
            can_rotate: false,
        });
        // 8 chars × 12px = 96 ≤ 100 → 12 is the largest fitting size.
        assert_eq!(decision, TextStrategyDecision::Shrink { font_size: 12 });
    }

    #[test]
    fn uniform_shrink_size_picks_smallest_per_segment_optimum() {
        // Two segments: a comfortable one and a tight one. Uniform shrink
        // must drop to the tighter cell's size so both fit.
        let easy = "短".repeat(4);
        let tight = "长".repeat(15);
        let segments = vec![(easy.as_str(), 200.0, 80.0), (tight.as_str(), 200.0, 80.0)];
        let uniform = uniform_shrink_size_for_segments(segments.into_iter(), 28, 10, 4);
        // Tight segment: 15*fs ≤ 200 → fs ≤ 13. Easy segment fits at 28.
        // Uniform = min(13, 28) = 13.
        assert_eq!(uniform, 13);
    }

    #[test]
    fn uniform_shrink_size_with_no_segments_returns_font_size() {
        let segments: Vec<(&str, f32, f32)> = Vec::new();
        let uniform = uniform_shrink_size_for_segments(segments.into_iter(), 24, 12, 4);
        assert_eq!(uniform, 24);
    }

    #[test]
    fn forced_newline_returns_normal_when_block_fits_both_axes() {
        // Two-line label that fits in width and height stays Normal.
        let decision = choose_text_strategy(TextStrategyInput {
            overflow: progressbar_schema::OverflowMode::Auto,
            text: "ab\ncd",
            rect_width_px: 200.0,
            rect_height_px: 80.0,
            font_size: 20,
            min_font_size: 12,
            line_spacing: 4,
            can_rotate: false,
        });
        assert_eq!(decision, TextStrategyDecision::Normal { font_size: 20 });
    }
}
