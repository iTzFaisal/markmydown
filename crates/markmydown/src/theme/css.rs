//! CSS generation from theme tokens.
//!
//! The exported stylesheet uses the same tokens as the in-app preview, so a
//! theme change is reflected by the next export.

use iced::Color;

use super::tokens::{ThemeTokens, Typography};

/// Builds the standalone stylesheet for an exported document.
pub fn stylesheet(tokens: &ThemeTokens, typography: &Typography) -> String {
    let font = sanitize_font_family(&typography.font_family);
    let max_width = if typography.max_width > 0.0 {
        format!("{}px", typography.max_width.clamp(320.0, 2400.0))
    } else {
        String::from("none")
    };

    format!(
        r#":root {{
  --mmd-bg: {bg};
  --mmd-surface: {surface};
  --mmd-surface-raised: {surface_raised};
  --mmd-text: {text};
  --mmd-muted: {muted};
  --mmd-accent: {accent};
  --mmd-border: {border};
  --mmd-selection: {selection};
  --mmd-code-bg: {code_bg};
  --mmd-code-text: {code_text};
  --mmd-link: {link};
  --mmd-heading: {heading};
  --mmd-quote: {quote};
  --mmd-success: {success};
  --mmd-warning: {warning};
  --mmd-error: {error};
  --mmd-font: {font};
  --mmd-font-size: {font_size}px;
  --mmd-line-height: {line_height};
}}

* {{ box-sizing: border-box; }}

html, body {{
  margin: 0;
  padding: 0;
  background: var(--mmd-bg);
  color: var(--mmd-text);
  font-family: var(--mmd-font);
  font-size: var(--mmd-font-size);
  line-height: var(--mmd-line-height);
  -webkit-font-smoothing: antialiased;
}}

.mmd-document {{
  max-width: {max_width};
  margin: 0 auto;
  padding: 48px 32px 96px;
  overflow-wrap: break-word;
}}

h1, h2, h3, h4, h5, h6 {{
  color: var(--mmd-heading);
  line-height: 1.25;
  margin: 1.5em 0 0.55em;
  font-weight: 650;
}}
h1 {{ font-size: 2.1em; border-bottom: 1px solid var(--mmd-border); padding-bottom: 0.25em; }}
h2 {{ font-size: 1.6em; border-bottom: 1px solid var(--mmd-border); padding-bottom: 0.2em; }}
h3 {{ font-size: 1.3em; }}
h4 {{ font-size: 1.12em; }}
h5, h6 {{ font-size: 1em; color: var(--mmd-muted); }}

p {{ margin: 0 0 1em; }}
a {{ color: var(--mmd-link); text-decoration: none; }}
a:hover {{ text-decoration: underline; }}
strong {{ font-weight: 680; }}
del {{ color: var(--mmd-muted); }}
mark {{ background: var(--mmd-selection); color: var(--mmd-text); padding: 0 0.15em; }}
sub, sup {{ font-size: 0.75em; line-height: 0; }}

blockquote {{
  margin: 1em 0;
  padding: 0.2em 1em;
  color: var(--mmd-quote);
  border-left: 4px solid var(--mmd-accent);
  background: var(--mmd-surface);
}}

code {{
  font-family: ui-monospace, "SF Mono", "Cascadia Code", Menlo, Consolas, monospace;
  font-size: 0.88em;
  background: var(--mmd-code-bg);
  color: var(--mmd-code-text);
  border: 1px solid var(--mmd-border);
  border-radius: 4px;
  padding: 0.1em 0.32em;
}}

pre, .mmd-code-block {{
  background: var(--mmd-code-bg);
  color: var(--mmd-code-text);
  border: 1px solid var(--mmd-border);
  border-radius: 8px;
  padding: 16px;
  overflow-x: auto;
  line-height: 1.45;
  margin: 1em 0;
}}
pre code {{ border: 0; padding: 0; background: transparent; font-size: 0.86em; }}

ul, ol {{ margin: 0 0 1em; padding-left: 1.6em; }}
li {{ margin: 0.25em 0; }}
li.task-list-item {{ list-style: none; margin-left: -1.35em; }}
li.task-list-item input {{ margin-right: 0.45em; }}

table {{
  border-collapse: collapse;
  width: 100%;
  margin: 1em 0;
  font-size: 0.95em;
}}
th, td {{
  border: 1px solid var(--mmd-border);
  padding: 8px 12px;
  text-align: left;
  vertical-align: top;
}}
th {{ background: var(--mmd-surface-raised); font-weight: 650; }}
tr:nth-child(even) td {{ background: var(--mmd-surface); }}

hr {{ border: 0; border-top: 1px solid var(--mmd-border); margin: 2em 0; }}
img {{ max-width: 100%; height: auto; border-radius: 6px; }}
.mmd-missing-image {{
  display: inline-block;
  padding: 8px 12px;
  border: 1px dashed var(--mmd-border);
  border-radius: 6px;
  color: var(--mmd-muted);
  background: var(--mmd-surface);
}}

.mmd-math, .mmd-mermaid {{
  margin: 1em 0;
  padding: 12px;
  text-align: center;
  background: var(--mmd-surface);
  border: 1px solid var(--mmd-border);
  border-radius: 8px;
  overflow-x: auto;
}}
.mmd-error {{
  color: var(--mmd-error);
  border-color: var(--mmd-error);
  font-family: ui-monospace, monospace;
  font-size: 0.9em;
  white-space: pre-wrap;
}}
.mmd-footnotes {{ margin-top: 2.5em; border-top: 1px solid var(--mmd-border); font-size: 0.9em; }}

@media print {{
  :root {{ --mmd-bg: #ffffff; }}
  html, body {{ background: #ffffff; }}
  .mmd-document {{ max-width: none; padding: 0; }}
  pre, blockquote, table, img, .mmd-math, .mmd-mermaid {{ break-inside: avoid; }}
  h1, h2, h3, h4 {{ break-after: avoid; }}
  a {{ color: inherit; }}
}}"#,
        bg = css_color(tokens.background),
        surface = css_color(tokens.surface),
        surface_raised = css_color(tokens.surface_raised),
        text = css_color(tokens.text),
        muted = css_color(tokens.muted),
        accent = css_color(tokens.accent),
        border = css_color(tokens.border),
        selection = css_color(tokens.selection),
        code_bg = css_color(tokens.code_background),
        code_text = css_color(tokens.code_text),
        link = css_color(tokens.link),
        heading = css_color(tokens.heading),
        quote = css_color(tokens.quote),
        success = css_color(tokens.success),
        warning = css_color(tokens.warning),
        error = css_color(tokens.error),
        font = font,
        font_size = typography.font_size.clamp(9.0, 48.0),
        line_height = typography.line_height.clamp(1.0, 3.0),
        max_width = max_width,
    )
}

/// Converts an Iced color to a CSS color string.
pub fn css_color(color: Color) -> String {
    let red = (color.r.clamp(0.0, 1.0) * 255.0).round() as u8;
    let green = (color.g.clamp(0.0, 1.0) * 255.0).round() as u8;
    let blue = (color.b.clamp(0.0, 1.0) * 255.0).round() as u8;

    if color.a >= 1.0 {
        format!("#{red:02x}{green:02x}{blue:02x}")
    } else {
        format!(
            "rgba({red}, {green}, {blue}, {:.3})",
            color.a.clamp(0.0, 1.0)
        )
    }
}

/// Restricts a font family to safe CSS characters.
pub fn sanitize_font_family(family: &str) -> String {
    let cleaned: String = family
        .chars()
        .filter(|ch| {
            ch.is_alphanumeric()
                || matches!(ch, ' ' | ',' | '-' | '_' | '\'' | '"')
                || !ch.is_ascii()
        })
        .collect();

    let cleaned = cleaned.trim();
    let cleaned = if cleaned.is_empty() {
        "Georgia"
    } else {
        cleaned
    };

    // Quote each comma-separated family that is not already quoted.
    cleaned
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            if part.starts_with('"') || part.starts_with('\'') {
                part.to_string()
            } else {
                format!("\"{part}\"")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stylesheet_uses_theme_tokens_and_typography() {
        let tokens = ThemeTokens::dark();
        let typography = Typography::preview_default();
        let css = stylesheet(&tokens, &typography);

        assert!(css.contains(&css_color(tokens.background)));
        assert!(css.contains(&css_color(tokens.text)));
        assert!(css.contains("17px"));
        assert!(css.contains("760px"));
        assert!(css.contains("--mmd-font"));
    }

    #[test]
    fn font_family_is_sanitized() {
        assert_eq!(sanitize_font_family("Inter"), "\"Inter\"");
        assert_eq!(
            sanitize_font_family("Georgia, serif"),
            "\"Georgia\", \"serif\""
        );
        assert!(!sanitize_font_family("Bad; } body { display:none").contains(';'));
        assert_eq!(sanitize_font_family(""), "\"Georgia\"");
    }

    #[test]
    fn no_max_width_when_zero() {
        let mut typography = Typography::preview_default();
        typography.max_width = 0.0;
        let css = stylesheet(&ThemeTokens::light(), &typography);
        assert!(css.contains("max-width: none"));
    }
}
