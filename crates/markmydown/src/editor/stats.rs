//! Word, character, reading-time, and selection statistics.

/// Counts for a document or a selected range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextStats {
    pub words: usize,
    pub characters: usize,
    pub characters_without_spaces: usize,
    pub reading_minutes: usize,
}

impl TextStats {
    pub fn from_text(text: &str) -> Self {
        let words = count_words(text);
        let characters = text.chars().count();
        let characters_without_spaces = text.chars().filter(|ch| !ch.is_whitespace()).count();

        Self {
            words,
            characters,
            characters_without_spaces,
            reading_minutes: reading_minutes(words),
        }
    }
}

/// Counts words, treating each CJK ideograph or kana as one word.
pub fn count_words(text: &str) -> usize {
    let mut words = 0;
    let mut in_word = false;

    for ch in text.chars() {
        if is_cjk(ch) {
            words += 1;
            in_word = false;
        } else if ch.is_alphanumeric() || ch == '\'' || ch == '_' {
            if !in_word {
                words += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }

    words
}

/// Reads at the common long-form pace of 200 words per minute.
pub fn reading_minutes(words: usize) -> usize {
    if words == 0 {
        0
    } else {
        words.div_ceil(200).max(1)
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3040..=0x30FF        // Hiragana, Katakana
            | 0x3400..=0x4DBF  // CJK Extension A
            | 0x4E00..=0x9FFF  // CJK Unified Ideographs
            | 0xAC00..=0xD7AF  // Hangul syllables
            | 0xF900..=0xFAFF  // CJK Compatibility Ideographs
            | 0x20000..=0x2FA1F // CJK Extensions B–F
    )
}

/// Extracts text between two `(line, column)` positions. Columns are character
/// offsets, matching the editor's cursor model.
pub fn text_between(text: &str, start: (usize, usize), end: (usize, usize)) -> Option<String> {
    let (start, end) = normalize(start, end);
    let lines: Vec<&str> = text.split('\n').collect();

    if start.0 >= lines.len() || end.0 >= lines.len() {
        return None;
    }

    if start.0 == end.0 {
        let chars: Vec<char> = lines[start.0].chars().collect();
        if start.1 > chars.len() || end.1 > chars.len() {
            return None;
        }
        return Some(chars[start.1..end.1].iter().collect());
    }

    let mut result = String::new();
    for (index, line) in lines
        .iter()
        .enumerate()
        .skip(start.0)
        .take(end.0 - start.0 + 1)
    {
        let chars: Vec<char> = line.chars().collect();
        if index == start.0 {
            if start.1 > chars.len() {
                return None;
            }
            result.extend(&chars[start.1..]);
            result.push('\n');
        } else if index == end.0 {
            let end_col = end.1.min(chars.len());
            result.extend(&chars[..end_col]);
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    Some(result)
}

fn normalize(a: (usize, usize), b: (usize, usize)) -> ((usize, usize), (usize, usize)) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Counts stats for a selected range, falling back to the whole document when
/// the range is empty.
pub fn selection_stats(
    text: &str,
    start: (usize, usize),
    end: (usize, usize),
) -> Option<TextStats> {
    let selected = text_between(text, start, end)?;
    if selected.is_empty() {
        return None;
    }
    Some(TextStats::from_text(&selected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_words_and_characters() {
        let stats = TextStats::from_text("Hello, world!\nA second line.");

        assert_eq!(stats.words, 5);
        assert_eq!(stats.characters, 28);
        assert_eq!(stats.characters_without_spaces, 24);
        assert_eq!(stats.reading_minutes, 1);
    }

    #[test]
    fn reading_time_rounds_up() {
        assert_eq!(reading_minutes(0), 0);
        assert_eq!(reading_minutes(1), 1);
        assert_eq!(reading_minutes(200), 1);
        assert_eq!(reading_minutes(201), 2);
        assert_eq!(reading_minutes(1_000), 5);
    }

    #[test]
    fn cjk_characters_count_as_words() {
        assert_eq!(count_words("你好世界"), 4);
        assert_eq!(count_words("こんにちは"), 5);
        assert_eq!(count_words("hello 世界"), 3);
    }

    #[test]
    fn selection_text_and_stats_work_across_lines() {
        let text = "alpha beta\ngamma delta";
        let selected = text_between(text, (0, 6), (1, 5)).expect("selection");

        assert_eq!(selected, "beta\ngamma");
        let stats = selection_stats(text, (0, 6), (1, 5)).expect("stats");
        assert_eq!(stats.words, 2);
    }

    #[test]
    fn empty_selection_has_no_stats() {
        assert!(selection_stats("text", (0, 1), (0, 1)).is_none());
    }
}
