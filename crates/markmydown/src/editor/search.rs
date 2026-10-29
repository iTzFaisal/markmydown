//! Find and replace with case sensitivity, regular expressions, and
//! replace-all.
//!
//! Replace-all returns one [`TextEdit`] so the app can apply it as a single
//! undo step. Invalid regular expressions never touch the document.

use regex::{Regex, RegexBuilder};

use super::markdown::{Position, TextEdit};

/// Search options shared by find and replace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub regex: bool,
}

/// One match in document coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub start: Position,
    pub end: Position,
    pub text: String,
}

/// Finds every non-overlapping match.
pub fn find_all(
    text: &str,
    query: &str,
    options: SearchOptions,
) -> Result<Vec<SearchMatch>, SearchError> {
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let regex = build_regex(query, options)?;
    let matches = regex
        .find_iter(text)
        .filter(|matched| matched.end() > matched.start())
        .map(|matched| {
            let start_char = text[..matched.start()].chars().count();
            let end_char = text[..matched.end()].chars().count();
            SearchMatch {
                start: char_to_position(text, start_char),
                end: char_to_position(text, end_char),
                text: matched.as_str().to_string(),
            }
        })
        .collect();

    Ok(matches)
}

/// Finds the next match at or after `cursor`, wrapping to the start.
pub fn find_next(
    text: &str,
    cursor: Position,
    query: &str,
    options: SearchOptions,
) -> Result<Option<SearchMatch>, SearchError> {
    let matches = find_all(text, query, options)?;
    let first = matches.first().cloned();
    Ok(matches
        .into_iter()
        .find(|matched| matched.start >= cursor)
        .or(first))
}

/// Finds the previous match before `cursor`, wrapping to the end.
pub fn find_previous(
    text: &str,
    cursor: Position,
    query: &str,
    options: SearchOptions,
) -> Result<Option<SearchMatch>, SearchError> {
    let matches = find_all(text, query, options)?;
    let last = matches.last().cloned();
    Ok(matches
        .iter()
        .rev()
        .find(|matched| matched.end <= cursor)
        .cloned()
        .or(last))
}

/// Replaces the currently selected match. Capture-group references such as
/// `$1` work when regex search is enabled.
pub fn replace_match(
    text: &str,
    query: &str,
    matched: &SearchMatch,
    replacement: &str,
    options: SearchOptions,
) -> Result<TextEdit, SearchError> {
    let regex = build_regex(query, options)?;
    let replaced = regex.replace(&matched.text, replacement).into_owned();
    let chars: Vec<char> = text.chars().collect();

    let mut new_text = String::with_capacity(text.len() + replaced.len());
    let start = position_to_char(&chars, matched.start);
    let end = position_to_char(&chars, matched.end);
    new_text.extend(chars[..start].iter());
    new_text.push_str(&replaced);
    new_text.extend(chars[end..].iter());

    Ok(TextEdit {
        text: new_text,
        selection: None,
        cursor: matched.start,
    })
}

/// Replaces every match. Capture-group references such as `$1` work when
/// regex search is enabled.
pub fn replace_all(
    text: &str,
    query: &str,
    replacement: &str,
    options: SearchOptions,
) -> Result<Option<TextEdit>, SearchError> {
    if query.is_empty() {
        return Ok(None);
    }

    let regex = build_regex(query, options)?;
    if !regex.is_match(text) {
        return Ok(None);
    }

    let new_text = regex.replace_all(text, replacement).into_owned();
    if new_text == text {
        return Ok(None);
    }

    Ok(Some(TextEdit {
        text: new_text,
        selection: None,
        cursor: (0, 0),
    }))
}

fn build_regex(query: &str, options: SearchOptions) -> Result<Regex, SearchError> {
    let pattern = if options.regex {
        query.to_string()
    } else {
        regex::escape(query)
    };

    RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .multi_line(true)
        .dot_matches_new_line(true)
        .build()
        .map_err(|error| SearchError::InvalidPattern(error.to_string()))
}

fn position_to_char(chars: &[char], position: Position) -> usize {
    let mut line = 0;
    let mut column = 0;

    for (index, ch) in chars.iter().enumerate() {
        if (line, column) >= position {
            return index;
        }
        if *ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }

    chars.len()
}

fn char_to_position(text: &str, char_index: usize) -> Position {
    let mut line = 0;
    let mut column = 0;

    for (index, ch) in text.chars().enumerate() {
        if index >= char_index {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }

    (line, column)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SearchError {
    #[error("invalid regular expression: {0}")]
    InvalidPattern(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_case_insensitively_by_default() {
        let matches = find_all("One one ONE", "one", SearchOptions::default()).expect("search");

        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].text, "One");
    }

    #[test]
    fn respects_case_sensitivity() {
        let matches = find_all(
            "One one ONE",
            "one",
            SearchOptions {
                case_sensitive: true,
                regex: false,
            },
        )
        .expect("search");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "one");
    }

    #[test]
    fn reports_line_and_column_positions() {
        let matches =
            find_all("alpha\nbeta gamma", "gamma", SearchOptions::default()).expect("search");

        assert_eq!(matches[0].start, (1, 5));
        assert_eq!(matches[0].end, (1, 10));
    }

    #[test]
    fn regular_expressions_and_capture_groups_work() {
        let matches = find_all(
            "a1 b2",
            r"([a-z])(\d)",
            SearchOptions {
                case_sensitive: true,
                regex: true,
            },
        )
        .expect("search");
        assert_eq!(matches.len(), 2);

        let edit = replace_all(
            "a1 b2",
            r"([a-z])(\d)",
            "$2$1",
            SearchOptions {
                case_sensitive: true,
                regex: true,
            },
        )
        .expect("replace")
        .expect("changed");

        assert_eq!(edit.text, "1a 2b");
        assert!(edit.selection.is_none());
    }

    #[test]
    fn replaces_one_match_and_supports_capture_groups() {
        let options = SearchOptions {
            case_sensitive: true,
            regex: true,
        };
        let matches = find_all("a1 b2 c3", r"([a-z])(\d)", options).expect("search");
        let edit = replace_match("a1 b2 c3", r"([a-z])(\d)", &matches[1], "$2$1", options)
            .expect("replace");

        assert_eq!(edit.text, "a1 2b c3");
        assert_eq!(edit.cursor, (0, 3));
    }

    #[test]
    fn replace_match_rejects_an_invalid_expression() {
        let matched = SearchMatch {
            start: (0, 0),
            end: (0, 1),
            text: "[".into(),
        };
        let error = replace_match(
            "[",
            "[",
            &matched,
            "x",
            SearchOptions {
                case_sensitive: true,
                regex: true,
            },
        )
        .expect_err("the invalid pattern must be reported");

        assert!(matches!(error, SearchError::InvalidPattern(_)));
    }

    #[test]
    fn invalid_regex_makes_no_replacement() {
        let error = replace_all(
            "text",
            "[",
            "x",
            SearchOptions {
                case_sensitive: true,
                regex: true,
            },
        )
        .expect_err("invalid");

        assert!(matches!(error, SearchError::InvalidPattern(_)));
    }

    #[test]
    fn no_matches_returns_none() {
        let edit = replace_all("text", "missing", "x", SearchOptions::default()).expect("replace");

        assert!(edit.is_none());
    }

    #[test]
    fn next_and_previous_wrap() {
        let options = SearchOptions::default();
        let next = find_next("one two one", (0, 4), "one", options)
            .expect("next")
            .expect("match");
        assert_eq!(next.start, (0, 8));

        let previous = find_previous("one two one", (0, 0), "one", options)
            .expect("previous")
            .expect("match");
        assert_eq!(previous.start, (0, 8));
    }
}
