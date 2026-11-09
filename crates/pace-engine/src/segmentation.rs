//! Script segmentation (`SCR-01`, `SCR-02`, `SCR-09`).
//!
//! Splits raw script text into ordered sections separated by one or more blank
//! (or whitespace-only) lines, enforces the 60,000-character limit, and calculates
//! word counts and estimated spoken duration at 150 words per minute.

use serde::{Deserialize, Serialize};

/// Maximum script length allowed by `SCR-01` (60,000 Unicode characters).
pub const MAX_SCRIPT_CHARS: usize = 60_000;

/// Default speaking rate used before narration exists (`SCR-09`: 150 words/minute).
pub const DEFAULT_WORDS_PER_MINUTE: u64 = 150;

/// A single segmented section extracted from a script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptSection {
    /// Zero-based index of the section in the script (`0..N`).
    pub index: usize,
    /// Cleaned text of the section (leading/trailing whitespace trimmed).
    pub text: String,
    /// Number of whitespace-separated words in this section (`SCR-09`).
    pub word_count: usize,
    /// Expected spoken length in milliseconds at 150 WPM (`SCR-09`).
    pub estimated_duration_ms: u64,
}

/// Errors that can occur when validating and segmenting a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentationError {
    /// Script exceeds `MAX_SCRIPT_CHARS` (60,000 characters, `SCR-01`).
    ScriptTooLong { char_count: usize, max_chars: usize },
}

/// Splits raw script text into section strings at one or more blank lines (`SCR-02`).
///
/// Rules (`SCR-02`):
/// - One or more blank lines start a new section.
/// - Lines containing only spaces or tabs count as blank lines.
/// - Windows (`\r\n`) and Unix (`\n`) line endings are handled identically.
/// - Leading and trailing spaces on each line (and each section) are trimmed.
/// - Consecutive non-blank lines within the same paragraph are joined with `\n`.
pub fn split_script(raw_text: &str) -> Vec<String> {
    let mut sections: Vec<String> = Vec::new();
    let mut current_lines: Vec<&str> = Vec::new();

    for line in raw_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current_lines.is_empty() {
                sections.push(current_lines.join("\n"));
                current_lines.clear();
            }
        } else {
            current_lines.push(trimmed);
        }
    }

    if !current_lines.is_empty() {
        sections.push(current_lines.join("\n"));
    }

    sections
}

/// Counts words and calculates the estimated spoken duration in milliseconds
/// at 150 words per minute (`SCR-09`: `word_count * 60_000 / 150` = `word_count * 400` ms).
pub fn estimate_spoken_duration_ms(text: &str) -> (usize, u64) {
    let word_count = text.split_whitespace().count();
    let estimated_duration_ms = (word_count as u64 * 60_000) / DEFAULT_WORDS_PER_MINUTE;
    (word_count, estimated_duration_ms)
}

/// Validates script length (`SCR-01`) and returns rich `ScriptSection` items (`SCR-02`, `SCR-09`).
pub fn segment_script(raw_text: &str) -> Result<Vec<ScriptSection>, SegmentationError> {
    let char_count = raw_text.chars().count();
    if char_count > MAX_SCRIPT_CHARS {
        return Err(SegmentationError::ScriptTooLong {
            char_count,
            max_chars: MAX_SCRIPT_CHARS,
        });
    }

    let sections = split_script(raw_text)
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let (word_count, estimated_duration_ms) = estimate_spoken_duration_ms(&text);
            ScriptSection {
                index,
                text,
                word_count,
                estimated_duration_ms,
            }
        })
        .collect();

    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_standard_paragraphs_and_trims_spaces() {
        let script = "  First paragraph with extra spaces.  \n\n  Second paragraph here. \n\nThird paragraph.";
        let sections = split_script(script);
        assert_eq!(
            sections,
            vec![
                "First paragraph with extra spaces.",
                "Second paragraph here.",
                "Third paragraph.",
            ]
        );
    }

    #[test]
    fn handles_windows_crlf_line_endings() {
        let script = "Scene one text.\r\n\r\nScene two text.\r\nStill scene two.\r\n\r\nScene three text.";
        let sections = split_script(script);
        assert_eq!(
            sections,
            vec![
                "Scene one text.",
                "Scene two text.\nStill scene two.",
                "Scene three text.",
            ]
        );
    }

    #[test]
    fn treats_whitespace_only_lines_as_blank_lines() {
        let script = "Paragraph A\n   \t  \nParagraph B\n \r\n \t \r\nParagraph C";
        let sections = split_script(script);
        assert_eq!(sections, vec!["Paragraph A", "Paragraph B", "Paragraph C"]);
    }

    #[test]
    fn collapses_several_blank_lines_in_a_row() {
        let script = "\n\n   \nFirst section\n\n\n\n   \n\nSecond section\n\n\n";
        let sections = split_script(script);
        assert_eq!(sections, vec!["First section", "Second section"]);
    }

    #[test]
    fn handles_single_long_paragraph_and_empty_input() {
        let single = "This is one unbroken paragraph with no blank lines at all. It should stay a single section.";
        assert_eq!(split_script(single), vec![single]);

        assert!(split_script("").is_empty());
        assert!(split_script("   \n  \t \r\n  ").is_empty());
    }

    #[test]
    fn accepts_60000_chars_and_rejects_60001_chars() {
        // Build a script of exactly 60,000 characters across multiple sections
        let paragraph = "word ".repeat(200); // 1,000 chars per paragraph
        let mut script_60k = String::with_capacity(MAX_SCRIPT_CHARS);
        while script_60k.len() + paragraph.len() + 2 <= MAX_SCRIPT_CHARS {
            if !script_60k.is_empty() {
                script_60k.push_str("\n\n");
            }
            script_60k.push_str(&paragraph);
        }
        let remaining = MAX_SCRIPT_CHARS - script_60k.len();
        if remaining > 0 {
            script_60k.push_str(&"a".repeat(remaining));
        }
        assert_eq!(script_60k.chars().count(), MAX_SCRIPT_CHARS);

        let result_ok = segment_script(&script_60k);
        assert!(result_ok.is_ok());
        let sections = result_ok.unwrap();
        assert!(!sections.is_empty());

        // Adding 1 more character must fail with ScriptTooLong
        let mut script_over = script_60k;
        script_over.push('x');
        assert_eq!(
            segment_script(&script_over),
            Err(SegmentationError::ScriptTooLong {
                char_count: MAX_SCRIPT_CHARS + 1,
                max_chars: MAX_SCRIPT_CHARS,
            })
        );
    }

    #[test]
    fn calculates_word_count_and_150_wpm_duration() {
        // 150 words at 150 WPM = exactly 60,000 ms (60 seconds)
        let text_150_words = vec!["hello"; 150].join(" ");
        let sections = segment_script(&text_150_words).expect("valid script");
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].index, 0);
        assert_eq!(sections[0].word_count, 150);
        assert_eq!(sections[0].estimated_duration_ms, 60_000);
    }
}
