//! Integration coverage for `specs/markdown-editor/spec.md`.
//!
//! The application adapter is exercised directly: formatting, smart editing,
//! search, focus, and the layout settings all flow through the same public
//! surface the Iced `update` function uses.

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use iced_code_editor::Message as EditorMessage;
use markmydown::app::LayoutMode;
use markmydown::config::settings::Settings;
use markmydown::editor::EditorPane;
use markmydown::editor::markdown::FormatAction;
use markmydown::editor::outline;
use markmydown::editor::search::{self, SearchOptions};
use markmydown::editor::stats;
use markmydown::theme::tokens::{ThemeTokens, Typography};

/// The editor uses a process-global focused-editor id. Serialize tests that
/// create editors so a parallel test cannot steal focus mid-assertion.
fn editor_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn pane(text: &str) -> EditorPane {
    let mut editor = EditorPane::new(text, &ThemeTokens::light(), &Typography::editor_default());
    editor.focus();
    editor
}

#[test]
fn typing_selection_undo_redo_and_paste_flow_through_the_adapter() {
    let _guard = editor_lock();
    let mut editor = pane("hello");

    let _ = editor.update(EditorMessage::End(false));
    let _ = editor.update(EditorMessage::CharacterInput('!'));
    assert_eq!(editor.content(), "hello!");

    let _ = editor.update(EditorMessage::Undo);
    assert_eq!(editor.content(), "hello");
    let _ = editor.update(EditorMessage::Redo);
    assert_eq!(editor.content(), "hello!");

    let _ = editor.select_range((0, 0), (0, 5));
    assert_eq!(editor.selected_text().as_deref(), Some("hello"));

    let _ = editor.update(EditorMessage::Paste("world".into()));
    assert_eq!(editor.content(), "world!");

    let _ = editor.select_range((0, 0), (0, 5));
    let _ = editor.update(EditorMessage::Cut);
    assert_eq!(editor.content(), "!");
    let _ = editor.update(EditorMessage::Undo);
    assert_eq!(editor.content(), "world!");
}

#[test]
fn ime_composition_is_committed_once_for_arabic_and_cjk() {
    let _guard = editor_lock();
    let mut editor = pane("");

    let _ = editor.update(EditorMessage::ImeCommit("مرحبا".into()));
    assert_eq!(editor.content(), "مرحبا");

    let _ = editor.update(EditorMessage::ImeCommit(" 世界".into()));
    assert_eq!(editor.content(), "مرحبا 世界");
    assert_eq!(editor.content().matches("مرحبا").count(), 1);
    assert_eq!(editor.content().matches('世').count(), 1);
}

#[test]
fn formatting_actions_produce_the_expected_markdown() {
    let _guard = editor_lock();
    let mut bold = pane("hello world");
    let _ = bold.select_range((0, 6), (0, 11));
    let _ = bold.apply_format(FormatAction::Bold);
    assert_eq!(bold.content(), "hello **world**");

    let mut italic = pane("text");
    let _ = italic.select_range((0, 0), (0, 4));
    let _ = italic.apply_format(FormatAction::Italic);
    assert_eq!(italic.content(), "_text_");

    let mut heading = pane("Title");
    let _ = heading.apply_format(FormatAction::Heading(2));
    assert_eq!(heading.content(), "## Title");

    let mut strike = pane("text");
    let _ = strike.select_range((0, 0), (0, 4));
    let _ = strike.apply_format(FormatAction::Strikethrough);
    assert_eq!(strike.content(), "~~text~~");

    let mut bullets = pane("one\ntwo");
    let _ = bullets.select_range((0, 0), (1, 3));
    let _ = bullets.apply_format(FormatAction::BulletList);
    assert_eq!(bullets.content(), "- one\n- two");

    let mut ordered = pane("one\ntwo");
    let _ = ordered.select_range((0, 0), (1, 3));
    let _ = ordered.apply_format(FormatAction::OrderedList);
    assert_eq!(ordered.content(), "1. one\n2. two");

    let mut task = pane("one");
    let _ = task.apply_format(FormatAction::TaskList);
    assert_eq!(task.content(), "- [ ] one");

    let mut quote = pane("one\ntwo");
    let _ = quote.select_range((0, 0), (1, 3));
    let _ = quote.apply_format(FormatAction::Blockquote);
    assert_eq!(quote.content(), "> one\n> two");

    let mut code = pane("let x = 1;");
    let _ = code.select_range((0, 0), (0, 10));
    let _ = code.apply_format(FormatAction::CodeBlock);
    assert_eq!(code.content(), "```\nlet x = 1;\n```");
}

#[test]
fn table_link_and_image_match_the_spec_scenarios() {
    let _guard = editor_lock();
    let mut table = pane("");
    let _ = table.apply_format(FormatAction::Table);
    assert!(table.content().contains("| Column 1 | Column 2 |"));
    assert_eq!(table.cursor(), (0, 2));

    let mut link = pane("docs");
    let _ = link.select_range((0, 0), (0, 4));
    let _ = link.insert_link("https://example.com", false);
    assert_eq!(link.content(), "[docs](https://example.com)");

    let mut placeholder = pane("docs");
    let _ = placeholder.select_range((0, 0), (0, 4));
    let _ = placeholder.insert_link("", false);
    assert_eq!(placeholder.content(), "[docs](url)");
    assert_eq!(placeholder.selection(), Some(((0, 7), (0, 10))));

    let mut image = pane("alt");
    let _ = image.select_range((0, 0), (0, 3));
    let _ = image.insert_link("image.png", true);
    assert_eq!(image.content(), "![alt](image.png)");
}

#[test]
fn smart_editing_continues_indents_and_links_pasted_urls() {
    let _guard = editor_lock();
    let mut list = pane("- item");
    let _ = list.update(EditorMessage::End(false));
    let _ = list.update(EditorMessage::Enter);
    assert_eq!(list.content(), "- item\n- ");

    let mut indent = pane("- item");
    let _ = indent.update(EditorMessage::End(false));
    let _ = indent.update(EditorMessage::Tab);
    assert_eq!(indent.content(), "    - item");

    let _ = indent.update(EditorMessage::FocusNavigationShiftTab);
    assert_eq!(indent.content(), "- item");

    let mut pasted = pane("label");
    let _ = pasted.select_range((0, 0), (0, 5));
    let _ = pasted.update(EditorMessage::Paste("https://example.com".into()));
    assert_eq!(pasted.content(), "[label](https://example.com)");
}

#[test]
fn search_supports_case_regex_navigation_and_single_step_replace_all() {
    let _guard = editor_lock();
    let text = "One one ONE\na1 b2";

    let insensitive = search::find_all(text, "one", SearchOptions::default()).expect("search");
    assert_eq!(insensitive.len(), 3);

    let sensitive = search::find_all(
        text,
        "one",
        SearchOptions {
            case_sensitive: true,
            regex: false,
        },
    )
    .expect("search");
    assert_eq!(sensitive.len(), 1);

    let next = search::find_next(text, (0, 0), "one", SearchOptions::default())
        .expect("next")
        .expect("match");
    assert_eq!(next.start, (0, 0));
    let previous = search::find_previous(text, (0, 0), "one", SearchOptions::default())
        .expect("previous")
        .expect("match");
    assert_eq!(previous.start, (0, 8));

    let mut editor = pane("one two one");
    let _ = editor
        .replace_all("one", "1", SearchOptions::default())
        .expect("replace");
    assert_eq!(editor.content(), "1 two 1");
    let _ = editor.update(EditorMessage::Undo);
    assert_eq!(editor.content(), "one two one");

    let before = editor.content();
    let error = editor
        .replace_all(
            "[",
            "x",
            SearchOptions {
                case_sensitive: true,
                regex: true,
            },
        )
        .expect_err("invalid regex");
    assert!(error.to_string().contains("invalid regular expression"));
    assert_eq!(editor.content(), before);
}

#[test]
fn outline_and_stats_cover_navigation_and_selection_counts() {
    let source = "---\ntitle: Notes\n---\n# One\n\ntext\n\n## Two\n";
    let headings = outline::headings(source);
    assert_eq!(headings.len(), 2);
    assert_eq!(headings[0].line, 3);
    assert_eq!(headings[1].text, "Two");

    let mut editor = pane(source);
    let _ = editor.goto_position((headings[1].line, 0));
    assert_eq!(editor.cursor(), (7, 0));

    let stats = stats::TextStats::from_text("Hello world");
    assert_eq!(stats.words, 2);
    assert_eq!(stats.characters, 11);
    assert_eq!(stats.reading_minutes, 1);

    let selected = stats::selection_stats("alpha beta\ngamma", (0, 0), (1, 5)).expect("selection");
    assert_eq!(selected.words, 3);
}

#[test]
fn focus_mode_toggles_and_restores_the_editor() {
    let _guard = editor_lock();
    let mut editor = pane("# Title\n\nBody");
    assert!(!editor.focus_mode());

    editor.set_focus_mode(true);
    assert!(editor.focus_mode());
    let _ = editor.goto_position((2, 4));
    let _ = editor.update(EditorMessage::CharacterInput('!'));
    assert!(editor.content().ends_with('!'));

    editor.set_focus_mode(false);
    assert!(!editor.focus_mode());
    assert_eq!(editor.content(), "# Title\n\nBody!");
}

#[test]
fn layout_modes_round_trip_and_split_ratio_persists() {
    for layout in LayoutMode::ALL {
        assert_eq!(LayoutMode::from_settings(layout.as_settings()), layout);
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("settings.json");
    let settings = Settings {
        layout: String::from("editor-only"),
        split_ratio: 0.37,
        outline_visible: false,
        focus_mode: true,
        ..Settings::default()
    };
    settings.save(&path).expect("save");
    let loaded = Settings::load(&path).settings;

    assert_eq!(loaded.layout, "editor-only");
    assert!((loaded.split_ratio - 0.37).abs() < f32::EPSILON);
    assert!(!loaded.outline_visible);
    assert!(loaded.focus_mode);
}

#[test]
fn a_one_megabyte_document_stays_within_the_interaction_budget() {
    let _guard = editor_lock();
    let line = "## Heading with **bold** and [a link](https://example.com)\n\n";
    let mut source = String::with_capacity(1_100_000);
    while source.len() < 1_000_000 {
        source.push_str(line);
    }

    let started = Instant::now();
    let mut editor = pane(&source);
    let base_len = source.len();
    let _ = editor.select_range((0, 0), (0, 10));
    let _ = editor.update(EditorMessage::End(false));
    let _ = editor.update(EditorMessage::CharacterInput('x'));
    let _ = editor.update(EditorMessage::Undo);
    let elapsed = started.elapsed();

    assert!(editor.content().len() >= 1_000_000);
    assert_eq!(editor.content().len(), base_len);
    assert!(
        elapsed.as_secs_f64() < 30.0,
        "1 MB editor operations took {elapsed:?}, which is outside the responsive-test budget"
    );
}
