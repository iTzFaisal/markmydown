//! HTML sanitization for preview and export.
//!
//! Scripts, event handlers, and unsafe URL schemes are removed. A small set of
//! safe presentational tags remains so `<kbd>`, `<br>`, `<mark>`, and similar
//! authoring survives.

use std::collections::HashSet;

use ammonia::Builder;

/// Sanitizes a block of HTML with a conservative allow-list.
pub fn sanitize_html(html: &str) -> String {
    Builder::default()
        .add_tags(&[
            "br",
            "kbd",
            "mark",
            "sub",
            "sup",
            "del",
            "ins",
            "small",
            "span",
            "p",
            "div",
            "blockquote",
            "ul",
            "ol",
            "li",
            "table",
            "thead",
            "tbody",
            "tr",
            "th",
            "td",
            "hr",
            "code",
            "pre",
        ])
        .link_rel(Some("noopener noreferrer"))
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        .clean(html)
        .to_string()
}

/// Sanitizes inline HTML and collapses it to a single line.
pub fn sanitize_inline(html: &str) -> String {
    sanitize_html(html).replace('\n', " ")
}

/// Converts sanitized inline HTML to a small set of styled spans.
///
/// This is intentionally not a full HTML renderer: the preview only supports
/// the safe inline tags that make sense in prose (`br`, `kbd`, `mark`, `sub`,
/// `sup`, `del`, `code`, `strong`, `em`). Unknown tags are stripped.
pub fn inline_html_to_text(html: &str) -> String {
    let sanitized = sanitize_inline(html);
    strip_tags(&sanitized)
}

fn strip_tags(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_and_event_handlers_are_removed() {
        let html = r#"<p onclick="alert(1)">safe</p><script>alert(1)</script>"#;
        let cleaned = sanitize_html(html);

        assert!(cleaned.contains("safe"));
        assert!(!cleaned.contains("script"));
        assert!(!cleaned.contains("onclick"));
    }

    #[test]
    fn javascript_urls_are_removed() {
        let cleaned = sanitize_html(r#"<a href="javascript:alert(1)">click</a>"#);

        assert!(!cleaned.to_lowercase().contains("javascript:"));
        assert!(cleaned.contains("click"));
    }

    #[test]
    fn safe_inline_html_survives() {
        let cleaned = sanitize_inline("Press <kbd>Cmd</kbd><br>then <mark>save</mark>");

        assert!(cleaned.contains("<kbd>"));
        assert!(cleaned.contains("<br"));
        assert!(cleaned.contains("<mark>"));
    }

    #[test]
    fn inline_text_strips_tags() {
        assert_eq!(
            inline_html_to_text("Press <kbd>Cmd</kbd> now"),
            "Press Cmd now"
        );
    }
}
