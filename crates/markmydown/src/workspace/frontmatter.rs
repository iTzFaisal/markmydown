//! YAML front matter parsing and metadata exposure.
//!
//! Valid front matter is removed from the rendered body but kept available to
//! the preview, export, and document title. Malformed front matter is treated
//! as ordinary text and reported instead of silently disappearing.

use serde_yaml_ng::Value;

/// A parsed front matter block and the body that follows it.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontMatter {
    raw: Option<String>,
    metadata: Option<Value>,
    body: String,
    /// Byte offset of `body` in the original source.
    body_offset: usize,
    error: Option<String>,
}

impl FrontMatter {
    /// Parses optional leading YAML front matter.
    pub fn parse(source: &str) -> Self {
        let Some((raw, yaml, body_offset)) = split(source) else {
            if starts_with_delimiter(source) {
                return Self {
                    raw: Some(source.to_string()),
                    metadata: None,
                    body: source.to_string(),
                    body_offset: 0,
                    error: Some(String::from("unterminated YAML front matter")),
                };
            }

            return Self {
                raw: None,
                metadata: None,
                body: source.to_string(),
                body_offset: 0,
                error: None,
            };
        };

        match serde_yaml_ng::from_str::<Value>(&yaml) {
            Ok(metadata) => Self {
                raw: Some(raw),
                metadata: Some(metadata),
                body: source[body_offset..].to_string(),
                body_offset,
                error: None,
            },
            Err(error) => Self {
                raw: Some(raw),
                metadata: None,
                body: source.to_string(),
                body_offset: 0,
                error: Some(format!("invalid YAML front matter: {error}")),
            },
        }
    }

    pub fn has_front_matter(&self) -> bool {
        self.raw.is_some()
    }

    pub fn is_valid(&self) -> bool {
        self.raw.is_some() && self.error.is_none()
    }

    pub fn raw(&self) -> Option<&str> {
        self.raw.as_deref()
    }

    pub fn metadata(&self) -> Option<&Value> {
        self.metadata.as_ref()
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    /// Byte offset of the body in the original document.
    pub fn body_offset(&self) -> usize {
        self.body_offset
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn title(&self) -> Option<&str> {
        self.string_field("title")
    }

    pub fn date(&self) -> Option<&str> {
        self.string_field("date")
    }

    /// Tags can be a YAML list or a comma-separated string.
    pub fn tags(&self) -> Vec<String> {
        let Some(value) = self.metadata.as_ref().and_then(|value| value.get("tags")) else {
            return Vec::new();
        };

        match value {
            Value::Sequence(items) => items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_string)
                .collect(),
            Value::String(tags) => tags
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn string_field(&self, key: &str) -> Option<&str> {
        self.metadata.as_ref()?.get(key)?.as_str()
    }
}

/// Whether the document starts with a front-matter delimiter line.
fn starts_with_delimiter(source: &str) -> bool {
    source == "---" || source.starts_with("---\n") || source.starts_with("---\r\n")
}

/// Splits source into `(raw block, YAML body, byte offset after block)`.
fn split(source: &str) -> Option<(String, String, usize)> {
    let mut lines = source.split_inclusive('\n');
    let first = lines.next()?;

    if first.trim_end_matches(['\r', '\n']) != "---" {
        return None;
    }

    let raw_start = 0;
    let mut offset = first.len();
    let yaml_start = offset;
    let mut yaml_end = None;

    for line in lines {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" || trimmed == "..." {
            yaml_end = Some(offset);
            offset += line.len();
            break;
        }
        offset += line.len();
    }

    let yaml_end = yaml_end?;
    let raw_end = offset;
    Some((
        source[raw_start..raw_end].to_string(),
        source[yaml_start..yaml_end].to_string(),
        raw_end,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "---\ntitle: Release notes\ndate: 2026-10-29\ntags: [rust, markdown]\n---\n# Body\n\nText\n";

    #[test]
    fn valid_front_matter_is_hidden_from_the_body() {
        let front_matter = FrontMatter::parse(VALID);

        assert!(front_matter.is_valid());
        assert_eq!(front_matter.title(), Some("Release notes"));
        assert_eq!(front_matter.date(), Some("2026-10-29"));
        assert_eq!(front_matter.tags(), vec!["rust", "markdown"]);
        assert_eq!(front_matter.body(), "# Body\n\nText\n");
        assert_eq!(&VALID[front_matter.body_offset()..], front_matter.body());
    }

    #[test]
    fn comma_separated_tags_are_supported() {
        let source = "---\ntitle: T\ntags: rust, markdown, gui\n---\nBody";
        let front_matter = FrontMatter::parse(source);

        assert_eq!(front_matter.tags(), vec!["rust", "markdown", "gui"]);
    }

    #[test]
    fn invalid_yaml_is_kept_as_text_and_reported() {
        let source = "---\ntitle: [unclosed\n---\n# Body\n";
        let front_matter = FrontMatter::parse(source);

        assert!(!front_matter.is_valid());
        assert!(front_matter.error().is_some());
        assert_eq!(front_matter.body(), source);
        assert_eq!(front_matter.body_offset(), 0);
    }

    #[test]
    fn unterminated_front_matter_is_kept_as_text() {
        let source = "---\ntitle: Never closed\n# Body\n";
        let front_matter = FrontMatter::parse(source);

        assert!(!front_matter.is_valid());
        assert_eq!(front_matter.body(), source);
        assert!(front_matter.error().is_some());
    }

    #[test]
    fn document_without_front_matter_is_unchanged() {
        let source = "# Just a document\n";
        let front_matter = FrontMatter::parse(source);

        assert!(!front_matter.has_front_matter());
        assert!(!front_matter.is_valid());
        assert_eq!(front_matter.body(), source);
        assert!(front_matter.error().is_none());
    }

    #[test]
    fn empty_body_is_allowed() {
        let front_matter = FrontMatter::parse("---\ntitle: Empty\n---\n");

        assert!(front_matter.is_valid());
        assert_eq!(front_matter.body(), "");
    }

    #[test]
    fn yaml_document_terminator_closes_the_block() {
        let source = "---\ntitle: Dot\n...\nBody";
        let front_matter = FrontMatter::parse(source);

        assert!(front_matter.is_valid());
        assert_eq!(front_matter.body(), "Body");
    }
}
