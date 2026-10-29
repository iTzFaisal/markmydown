//! Behavioral tests for the appearance and UI specification.

use insta::assert_snapshot;
use markmydown::export::html::{self, HtmlOptions};
use markmydown::preview::parser;
use markmydown::theme::css::css_color;
use markmydown::theme::registry;
use markmydown::theme::tokens::{ThemeTokens, Typography};
use markmydown::ui::welcome;

fn html_options<'a>(theme: &'a ThemeTokens, typography: &'a Typography) -> HtmlOptions<'a> {
    HtmlOptions {
        theme,
        typography,
        base_dir: None,
        title: "Built-in document",
        embed_images: true,
    }
}

#[test]
fn cheat_sheet_renders_as_a_ui_snapshot() {
    let document = parser::parse(welcome::CHEAT_SHEET, 0);
    let theme = ThemeTokens::light();
    let typography = Typography::preview_default();
    let html = html::render_fragment(&document, &html_options(&theme, &typography));

    assert_snapshot!("cheat_sheet", html);
}

#[test]
fn welcome_document_demonstrates_offline_content() {
    let document = parser::parse(welcome::WELCOME, 0);
    let theme = ThemeTokens::light();
    let typography = Typography::preview_default();
    let html = html::render_fragment(&document, &html_options(&theme, &typography));

    assert!(html.contains("<h1>Welcome to markmydown</h1>"));
    assert!(html.contains("<strong>bold</strong>"));
    assert!(html.contains("<a href=\"https://commonmark.org\">links</a>"));
    assert!(html.contains("<table>"));
    assert!(html.contains("language-rust"));
    assert!(html.contains("mmd-math"));
    assert!(html.contains("mmd-mermaid"));
    assert!(!html.contains("<script"));
    assert!(!html.contains("src=\"http"));
    assert!(!html.contains("<link rel="));
}

#[test]
fn theme_tokens_snapshot_is_stable() {
    let mut summary = String::new();
    for theme in registry::all() {
        summary.push_str(&format!(
            "{}: {} / {} / {}\n",
            theme.id,
            css_color(theme.tokens.background),
            css_color(theme.tokens.text),
            css_color(theme.tokens.accent),
        ));
    }

    assert_snapshot!("theme_tokens", summary);
}

#[test]
fn gallery_has_light_dark_and_named_themes() {
    assert_eq!(registry::light().id, "light");
    assert_eq!(registry::dark().id, "dark");
    assert!(registry::gallery().count() >= 6);
    assert_eq!(registry::resolve("system", false).id, "light");
    assert_eq!(registry::resolve("system", true).id, "dark");
}

#[test]
fn typography_defaults_are_independent() {
    let editor = Typography::editor_default();
    let preview = Typography::preview_default();

    assert_ne!(editor.font_family, preview.font_family);
    assert_ne!(editor.font_size, preview.font_size);
    assert_ne!(preview.max_width, 0.0);
}

#[test]
fn every_theme_has_a_readable_contrast_pair() {
    for theme in registry::all() {
        assert_ne!(
            theme.tokens.text, theme.tokens.background,
            "{} has no contrast",
            theme.id
        );
        assert_ne!(
            theme.tokens.link, theme.tokens.background,
            "{} has an invisible link color",
            theme.id
        );
    }
}
