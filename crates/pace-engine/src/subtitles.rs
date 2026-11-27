//! Pure subtitle formatting, validation, split/merge, style rules, and music settings
//! (`SUB-01..05`, `MUS-01..06`, TDD Sections 11.5, 13.2, 13.5).
//!
//! Enforces broadcast subtitle rules:
//! - At most 2 lines per cue (`MAX_SUBTITLE_LINES = 2`).
//! - At most 42 characters per line for `16:9` (`32` for `9:16`).
//! - Cue durations between 1.0s (`1000 ms`) and 7.0s (`7000 ms`).

use crate::timing::time_ms_to_frame;
use serde::{Deserialize, Serialize};

pub const MAX_SUBTITLE_LINES: usize = 2;
pub const MAX_LINE_CHARS_16_9: usize = 42;
pub const MAX_LINE_CHARS_9_16: usize = 32;
pub const MIN_SUBTITLE_DURATION_MS: u64 = 1_000;
pub const MAX_SUBTITLE_DURATION_MS: u64 = 7_000;

fn default_true() -> bool {
    true
}
fn default_font_size_pct() -> u32 {
    100
}
fn default_text_color() -> String {
    "#FFFFFF".to_string()
}
fn default_outline_px() -> u32 {
    2
}
fn default_bg_opacity_pct() -> u32 {
    75
}
fn default_music_volume_db() -> f32 {
    -18.0
}
fn default_duck_amount_db() -> f32 {
    -10.0
}
fn default_loop_crossfade_ms() -> u32 {
    1_500
}
fn default_fade_in_ms() -> u32 {
    1_500
}
fn default_fade_out_ms() -> u32 {
    2_500
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleFont {
    #[default]
    Sans,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubtitlePosition {
    #[default]
    Bottom,
    Center,
    Top,
}

/// Visual styling for burned-in and previewed subtitles (`SUB-04`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleStyleSpec {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub font_family: SubtitleFont,
    #[serde(default = "default_font_size_pct")]
    pub font_size_pct: u32,
    #[serde(default)]
    pub position: SubtitlePosition,
    #[serde(default = "default_text_color")]
    pub text_color: String,
    #[serde(default = "default_outline_px")]
    pub outline_width_px: u32,
    #[serde(default = "default_bg_opacity_pct")]
    pub bg_opacity_pct: u32,
}

impl Default for SubtitleStyleSpec {
    fn default() -> Self {
        Self {
            enabled: true,
            font_family: SubtitleFont::Sans,
            font_size_pct: 100,
            position: SubtitlePosition::Bottom,
            text_color: "#FFFFFF".to_string(),
            outline_width_px: 2,
            bg_opacity_pct: 75,
        }
    }
}

/// A single timed subtitle cue (`SUB-01..03`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleCueSpec {
    #[serde(default)]
    pub id: String,
    pub position: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

impl SubtitleCueSpec {
    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }

    pub fn frame_range(&self, fps: u32) -> (u32, u32) {
        let s = time_ms_to_frame(self.start_ms, fps);
        let e = time_ms_to_frame(self.end_ms, fps).max(s + 1);
        (s, e)
    }
}

/// Background music mix and ducking configuration (`MUS-01..06`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MusicSettingsSpec {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub media_id: Option<String>,
    #[serde(default = "default_music_volume_db")]
    pub volume_db: f32,
    #[serde(default = "default_true")]
    pub ducking_enabled: bool,
    #[serde(default = "default_duck_amount_db")]
    pub duck_amount_db: f32,
    #[serde(default = "default_true")]
    pub loop_enabled: bool,
    #[serde(default = "default_loop_crossfade_ms")]
    pub loop_crossfade_ms: u32,
    #[serde(default = "default_fade_in_ms")]
    pub fade_in_ms: u32,
    #[serde(default = "default_fade_out_ms")]
    pub fade_out_ms: u32,
}

impl Default for MusicSettingsSpec {
    fn default() -> Self {
        Self {
            enabled: true,
            media_id: None,
            volume_db: -18.0,
            ducking_enabled: true,
            duck_amount_db: -10.0,
            loop_enabled: true,
            loop_crossfade_ms: 1_500,
            fade_in_ms: 1_500,
            fade_out_ms: 2_500,
        }
    }
}

/// Returns maximum characters per subtitle line (`42` for `16:9`, `32` for `9:16`).
pub fn max_line_chars_for_aspect(aspect_ratio: &str) -> usize {
    if aspect_ratio == "9:16" {
        MAX_LINE_CHARS_9_16
    } else {
        MAX_LINE_CHARS_16_9
    }
}

/// Wraps subtitle text into balanced lines respecting `max_line_chars_for_aspect` (`SUB-02`).
pub fn wrap_subtitle_lines(text: &str, aspect_ratio: &str) -> Vec<String> {
    let max_chars = max_line_chars_for_aspect(aspect_ratio);

    // If caller already inserted explicit line breaks, normalize each line.
    if text.contains('\n') {
        let explicit: Vec<String> = text
            .lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|l| !l.is_empty())
            .collect();
        if !explicit.is_empty() && explicit.len() <= MAX_SUBTITLE_LINES {
            if explicit.iter().all(|l| l.chars().count() <= max_chars) {
                return explicit;
            }
        }
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }

    let joined = words.join(" ");
    if joined.chars().count() <= max_chars || words.len() == 1 {
        return vec![joined];
    }

    // Find a balanced 2-line split where both lines fit <= max_chars (if possible).
    let mut best_split: Option<(usize, usize)> = None;
    for split_idx in 1..words.len() {
        let left = words[..split_idx].join(" ");
        let right = words[split_idx..].join(" ");
        let l_len = left.chars().count();
        let r_len = right.chars().count();
        if l_len <= max_chars && r_len <= max_chars {
            let diff = l_len.abs_diff(r_len);
            if best_split.map_or(true, |(_, best_diff)| diff < best_diff) {
                best_split = Some((split_idx, diff));
            }
        }
    }

    if let Some((idx, _)) = best_split {
        return vec![words[..idx].join(" "), words[idx..].join(" ")];
    }

    // Greedy fallback into lines of at most `max_chars`
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in words {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if candidate.chars().count() <= max_chars || current.is_empty() {
            current = candidate;
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Validates a subtitle cue against `SUB-02` (`<= 2` lines, `<= 42/32` chars/line, `1..=7s` duration).
pub fn validate_subtitle_cue(cue: &SubtitleCueSpec, aspect_ratio: &str) -> Vec<String> {
    let mut issues = Vec::new();
    let trimmed = cue.text.trim();
    if trimmed.is_empty() {
        issues.push("EMPTY_TEXT".to_string());
        return issues;
    }

    if cue.end_ms <= cue.start_ms {
        issues.push("INVALID_TIME_RANGE".to_string());
    } else {
        let dur = cue.duration_ms();
        if dur < MIN_SUBTITLE_DURATION_MS {
            issues.push("DURATION_TOO_SHORT".to_string());
        }
        if dur > MAX_SUBTITLE_DURATION_MS {
            issues.push("DURATION_TOO_LONG".to_string());
        }
    }

    let max_chars = max_line_chars_for_aspect(aspect_ratio);
    let lines = wrap_subtitle_lines(trimmed, aspect_ratio);
    if lines.len() > MAX_SUBTITLE_LINES {
        issues.push("TOO_MANY_LINES".to_string());
    }
    if lines.iter().any(|l| l.chars().count() > max_chars) {
        issues.push("LINE_TOO_LONG".to_string());
    }

    issues
}

/// Splits a subtitle cue into two consecutive cues at `split_ms` (or midpoint) (`SUB-03`).
pub fn split_subtitle_cue(
    cue: &SubtitleCueSpec,
    split_ms: Option<u64>,
    aspect_ratio: &str,
) -> Result<(SubtitleCueSpec, SubtitleCueSpec), String> {
    if cue.end_ms <= cue.start_ms + 400 {
        return Err("Cue is too short to split".to_string());
    }
    let words: Vec<&str> = cue.text.split_whitespace().collect();
    if words.len() < 2 {
        return Err("Cue must have at least two words to split".to_string());
    }

    let default_mid = cue.start_ms + (cue.end_ms - cue.start_ms) / 2;
    let mid_ms = split_ms
        .unwrap_or(default_mid)
        .clamp(cue.start_ms + 200, cue.end_ms - 200);

    let ratio = ((mid_ms - cue.start_ms) as f64) / ((cue.end_ms - cue.start_ms) as f64);
    let word_split = ((words.len() as f64 * ratio).round() as usize).clamp(1, words.len() - 1);

    let left_text = wrap_subtitle_lines(&words[..word_split].join(" "), aspect_ratio).join("\n");
    let right_text = wrap_subtitle_lines(&words[word_split..].join(" "), aspect_ratio).join("\n");

    let left = SubtitleCueSpec {
        id: cue.id.clone(),
        position: cue.position,
        start_ms: cue.start_ms,
        end_ms: mid_ms,
        text: left_text,
    };
    let right = SubtitleCueSpec {
        id: String::new(),
        position: cue.position + 1,
        start_ms: mid_ms,
        end_ms: cue.end_ms,
        text: right_text,
    };
    Ok((left, right))
}

/// Merges two adjacent subtitle cues into a single cue (`SUB-03`).
pub fn merge_subtitle_cues(
    first: &SubtitleCueSpec,
    second: &SubtitleCueSpec,
    aspect_ratio: &str,
) -> Result<SubtitleCueSpec, String> {
    let (a, b) = if first.start_ms <= second.start_ms {
        (first, second)
    } else {
        (second, first)
    };
    let combined_raw = format!(
        "{} {}",
        a.text.replace('\n', " ").trim(),
        b.text.replace('\n', " ").trim()
    );
    let wrapped = wrap_subtitle_lines(combined_raw.trim(), aspect_ratio).join("\n");
    Ok(SubtitleCueSpec {
        id: a.id.clone(),
        position: a.position.min(b.position),
        start_ms: a.start_ms,
        end_ms: b.end_ms.max(a.end_ms),
        text: wrapped,
    })
}

/// Finds the active subtitle cue (if any) at `frame_index` for `fps`.
pub fn active_subtitle_for_frame<'a>(
    cues: &'a [SubtitleCueSpec],
    frame_index: u32,
    fps: u32,
) -> Option<&'a SubtitleCueSpec> {
    let time_ms = ((frame_index as u64) * 1000) / (fps.max(1) as u64);
    cues.iter()
        .find(|c| time_ms >= c.start_ms && time_ms < c.end_ms && !c.text.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_rules_enforce_line_count_char_limits_and_durations() {
        // 16:9 allows up to 42 chars/line across at most 2 lines
        let wrapped_16_9 = wrap_subtitle_lines(
            "Before a single beam is lifted into place engineers map how wind and light move",
            "16:9",
        );
        assert_eq!(wrapped_16_9.len(), 2);
        assert!(wrapped_16_9.iter().all(|l| l.chars().count() <= 42));

        // 9:16 enforces 32 chars/line
        let wrapped_9_16 = wrap_subtitle_lines(
            "Before a single beam is lifted into place engineers map",
            "9:16",
        );
        assert_eq!(wrapped_9_16.len(), 2);
        assert!(wrapped_9_16.iter().all(|l| l.chars().count() <= 32));

        let valid_cue = SubtitleCueSpec {
            id: "c1".to_string(),
            position: 0,
            start_ms: 1_000,
            end_ms: 4_500,
            text: wrapped_16_9.join("\n"),
        };
        assert!(validate_subtitle_cue(&valid_cue, "16:9").is_empty());

        // Too short (< 1000ms) & too long (> 7000ms)
        let short_cue = SubtitleCueSpec {
            start_ms: 1_000,
            end_ms: 1_500,
            ..valid_cue.clone()
        };
        assert!(validate_subtitle_cue(&short_cue, "16:9").contains(&"DURATION_TOO_SHORT".to_string()));

        let long_cue = SubtitleCueSpec {
            start_ms: 1_000,
            end_ms: 9_000,
            ..valid_cue
        };
        assert!(validate_subtitle_cue(&long_cue, "16:9").contains(&"DURATION_TOO_LONG".to_string()));
    }

    #[test]
    fn split_and_merge_subtitle_cues_preserve_words_and_monotonic_bounds() {
        let cue = SubtitleCueSpec {
            id: "c0".to_string(),
            position: 2,
            start_ms: 2_000,
            end_ms: 6_000,
            text: "Cranes swing massive trusses above the streets while crews rivet each joint".to_string(),
        };

        let (left, right) = split_subtitle_cue(&cue, None, "16:9").expect("split cue");
        assert_eq!(left.start_ms, 2_000);
        assert_eq!(left.end_ms, right.start_ms);
        assert_eq!(right.end_ms, 6_000);
        assert!(!left.text.is_empty());
        assert!(!right.text.is_empty());

        let merged = merge_subtitle_cues(&left, &right, "16:9").expect("merge cues");
        assert_eq!(merged.start_ms, 2_000);
        assert_eq!(merged.end_ms, 6_000);
        assert_eq!(
            merged.text.replace('\n', " "),
            "Cranes swing massive trusses above the streets while crews rivet each joint"
        );
    }
}
