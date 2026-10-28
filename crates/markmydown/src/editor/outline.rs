//! Heading outline extraction.
//!
//! Supports ATX (`## Heading`) and setext (`Heading` followed by `===` or
//! `---`) headings while ignoring fenced code and YAML front matter.

/// A heading in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 1–6.
    pub level: u8,
    pub text: String,
    /// Zero-based source line.
    pub line: usize,
}

/// Extracts the outline for `source`.
pub fn headings(source: &str) -> Vec<Heading> {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut headings = Vec::new();
    let mut index = 0;
    let mut fence: Option<(char, usize)> = None;

    if lines.first().is_some_and(|line| line.trim() == "---") {
        index = 1;
        while index < lines.len() {
            let trimmed = lines[index].trim();
            index += 1;
            if trimmed == "---" || trimmed == "..." {
                break;
            }
        }
    }

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();

        if let Some((marker, length)) = fence {
            let count = trimmed.chars().take_while(|ch| *ch == marker).count();
            if count >= length && trimmed[count..].trim().is_empty() {
                fence = None;
            }
            index += 1;
            continue;
        }

        if let Some((marker, length)) = fence_start(trimmed) {
            fence = Some((marker, length));
            index += 1;
            continue;
        }

        if let Some((level, text)) = atx_heading(trimmed) {
            headings.push(Heading {
                level,
                text,
                line: index,
            });
            index += 1;
            continue;
        }

        if index + 1 < lines.len()
            && !line.trim().is_empty()
            && let Some(level) = setext_level(lines[index + 1].trim())
        {
            headings.push(Heading {
                level,
                text: line.trim().to_string(),
                line: index,
            });
            index += 2;
            continue;
        }

        index += 1;
    }

    headings
}

fn fence_start(line: &str) -> Option<(char, usize)> {
    if line.starts_with("```") {
        let length = line.chars().take_while(|ch| *ch == '`').count();
        Some(('`', length.max(3)))
    } else if line.starts_with("~~~") {
        let length = line.chars().take_while(|ch| *ch == '~').count();
        Some(('~', length.max(3)))
    } else {
        None
    }
}

fn atx_heading(line: &str) -> Option<(u8, String)> {
    let level = line.chars().take_while(|ch| *ch == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }

    let rest = &line[level..];
    if !rest.is_empty() && !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }

    let text = rest.trim().trim_end_matches('#').trim().to_string();
    Some((level as u8, text))
}

fn setext_level(line: &str) -> Option<u8> {
    if line.len() < 2 {
        return None;
    }
    if line.chars().all(|ch| ch == '=') {
        Some(1)
    } else if line.chars().all(|ch| ch == '-') {
        Some(2)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_atx_headings() {
        let source = "# One\n\ntext\n\n## Two ##\n###Three\n#### Four";
        let outline = headings(source);

        assert_eq!(
            outline,
            vec![
                Heading {
                    level: 1,
                    text: "One".into(),
                    line: 0
                },
                Heading {
                    level: 2,
                    text: "Two".into(),
                    line: 4
                },
                Heading {
                    level: 4,
                    text: "Four".into(),
                    line: 6
                },
            ]
        );
    }

    #[test]
    fn ignores_headings_inside_fenced_code() {
        let source = "# Real\n```md\n# Not a heading\n```\n## Also real";
        let outline = headings(source);

        assert_eq!(outline.len(), 2);
        assert_eq!(outline[0].text, "Real");
        assert_eq!(outline[1].text, "Also real");
    }

    #[test]
    fn ignores_front_matter() {
        let source = "---\ntitle: # not a heading\n---\n# Body";
        let outline = headings(source);

        assert_eq!(
            outline,
            vec![Heading {
                level: 1,
                text: "Body".into(),
                line: 3
            }]
        );
    }

    #[test]
    fn supports_setext_headings() {
        let source = "Title\n=====\n\nSubtitle\n--------";
        let outline = headings(source);

        assert_eq!(outline[0].level, 1);
        assert_eq!(outline[0].text, "Title");
        assert_eq!(outline[1].level, 2);
        assert_eq!(outline[1].text, "Subtitle");
    }

    #[test]
    fn empty_document_has_no_outline() {
        assert!(headings("").is_empty());
    }
}
