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
            let label = label_text.trim();
            if label.is_empty() {
                return Err(SegmentParseError::MissingLabel { line: line_number });
            }

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
                label: label.to_string(),
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
pub struct TextStrategyInput {
    pub overflow: progressbar_schema::OverflowMode,
    pub text_width_px: f32,
    pub rect_width_px: f32,
    pub font_size: u32,
    pub min_font_size: u32,
    pub can_rotate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStrategyDecision {
    Normal { font_size: u32 },
    Shrink { font_size: u32 },
    Ellipsis { font_size: u32 },
    Rotate { font_size: u32 },
    Scroll { font_size: u32 },
}

pub fn choose_text_strategy(input: TextStrategyInput) -> TextStrategyDecision {
    use progressbar_schema::OverflowMode;

    if input.text_width_px <= input.rect_width_px {
        return TextStrategyDecision::Normal {
            font_size: input.font_size,
        };
    }

    match input.overflow {
        OverflowMode::Shrink => TextStrategyDecision::Shrink {
            font_size: input.min_font_size,
        },
        OverflowMode::Ellipsis => TextStrategyDecision::Ellipsis {
            font_size: input.font_size,
        },
        OverflowMode::Rotate => TextStrategyDecision::Rotate {
            font_size: input.font_size,
        },
        OverflowMode::Scroll => TextStrategyDecision::Scroll {
            font_size: input.font_size,
        },
        OverflowMode::Auto => {
            let shrink_ratio = input.rect_width_px / input.text_width_px;
            let shrunk_size = ((input.font_size as f32 * shrink_ratio).floor() as u32)
                .clamp(input.min_font_size, input.font_size);
            if shrunk_size > input.min_font_size {
                return TextStrategyDecision::Shrink {
                    font_size: shrunk_size,
                };
            }
            if input.text_width_px <= input.rect_width_px * 1.8 {
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
        let decision = choose_text_strategy(TextStrategyInput {
            overflow: progressbar_schema::OverflowMode::Auto,
            text_width_px: 500.0,
            rect_width_px: 100.0,
            font_size: 28,
            min_font_size: 18,
            can_rotate: false,
        });
        assert_eq!(decision, TextStrategyDecision::Scroll { font_size: 18 });
    }

    #[test]
    fn auto_prefers_rotation_for_narrow_cells_when_allowed() {
        let decision = choose_text_strategy(TextStrategyInput {
            overflow: progressbar_schema::OverflowMode::Auto,
            text_width_px: 300.0,
            rect_width_px: 80.0,
            font_size: 28,
            min_font_size: 18,
            can_rotate: true,
        });
        assert_eq!(decision, TextStrategyDecision::Rotate { font_size: 18 });
    }

    #[test]
    fn explicit_scroll_uses_configured_font_size() {
        let decision = choose_text_strategy(TextStrategyInput {
            overflow: progressbar_schema::OverflowMode::Scroll,
            text_width_px: 300.0,
            rect_width_px: 80.0,
            font_size: 28,
            min_font_size: 18,
            can_rotate: true,
        });
        assert_eq!(decision, TextStrategyDecision::Scroll { font_size: 28 });
    }
}
