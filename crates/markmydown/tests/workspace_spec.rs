//! Integration coverage for `specs/document-workspace/spec.md`.
//!
//! These tests exercise the public workspace surface the application uses:
//! scanning a folder, tab/session state, file-safety behavior, import, and
//! front matter.

use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use markmydown::config::settings::Settings;
use markmydown::workspace::document::{Cursor, ExternalChange};
use markmydown::workspace::frontmatter::FrontMatter;
use markmydown::workspace::import::{self, ImportError};
use markmydown::workspace::recovery::RecoveryStore;
use markmydown::workspace::session::Session;
use markmydown::workspace::state::WorkspaceState;
use markmydown::workspace::tree::WorkspaceTree;

fn workspace_fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    fs::write(tmp.path().join("one.md"), "# One\n").expect("one");
    fs::write(tmp.path().join("two.markdown"), "# Two\n").expect("two");
    fs::write(tmp.path().join("three.mdown"), "# Three\n").expect("three");
    fs::write(tmp.path().join("notes.txt"), "plain\n").expect("txt");
    fs::write(tmp.path().join("image.png"), b"png").expect("png");
    fs::create_dir_all(tmp.path().join("notes")).expect("notes");
    fs::write(tmp.path().join("notes/nested.md"), "# Nested\n").expect("nested");
    tmp
}

fn state(root: &Path) -> WorkspaceState {
    let mut state = WorkspaceState::new(root.join("session.json"), root.join("recovery"));
    state.open_folder(root).expect("open workspace");
    state
}

#[test]
fn opens_a_folder_with_supported_documents_and_an_empty_state() {
    let tmp = workspace_fixture();
    let tree = WorkspaceTree::open(tmp.path()).expect("open tree");
    let names: Vec<_> = tree.files().iter().map(|node| node.name.as_str()).collect();

    assert!(names.contains(&"one.md"));
    assert!(names.contains(&"two.markdown"));
    assert!(names.contains(&"three.mdown"));
    assert!(names.contains(&"notes.txt"));
    assert!(names.contains(&"nested.md"));
    assert!(!names.contains(&"image.png"));

    let empty = tempfile::tempdir().expect("empty");
    let empty_tree = WorkspaceTree::open(empty.path()).expect("empty tree");
    assert!(empty_tree.files().is_empty());
}

#[test]
fn open_folder_is_remembered_for_the_next_launch() {
    let tmp = workspace_fixture();
    let settings_path = tmp.path().join("settings.json");
    let mut settings = Settings::default();
    settings.add_recent_workspace(tmp.path());
    settings.save(&settings_path).expect("save settings");

    let loaded = Settings::load(&settings_path).settings;

    assert_eq!(loaded.last_workspace.as_deref(), Some(tmp.path()));
    assert_eq!(
        loaded.recent_workspaces.first(),
        Some(&tmp.path().to_path_buf())
    );
}

#[test]
fn manages_files_and_folders_on_disk_and_in_the_tree() {
    let tmp = workspace_fixture();
    let mut state = state(tmp.path());
    let root = state.root().expect("root").to_path_buf();

    let id = state
        .create_document(&root, "new-document", "# New\n")
        .expect("create document");
    let created = root.join("new-document.md");
    assert!(created.is_file());
    assert!(state.tree().expect("tree").find(&created).is_some());
    assert_eq!(state.active_id(), Some(id));

    let renamed = state.rename_entry(&created, "renamed.md").expect("rename");
    assert!(!created.exists());
    assert!(renamed.is_file());
    assert_eq!(state.document(id).expect("document").path, renamed);
    assert_eq!(
        state.document(id).expect("document").display_title(),
        "renamed"
    );

    let moved = state
        .move_entry(&renamed, root.join("notes"))
        .expect("move");
    assert!(!renamed.exists());
    assert!(moved.is_file());
    assert_eq!(state.document(id).expect("document").path, moved);
    assert!(state.tree().expect("tree").find(&moved).is_some());

    let folder = state
        .create_folder(&root, "archive")
        .expect("create folder");
    assert!(folder.is_dir());
    assert!(state.tree().expect("tree").find(&folder).is_some());

    state.delete_entry(&moved).expect("delete");
    assert!(!moved.exists());
    assert!(state.document(id).is_none());
    assert!(state.tree().expect("tree").find(&moved).is_none());
}

#[test]
fn tabs_show_dirty_state_and_restore_the_whole_session() {
    let tmp = workspace_fixture();
    let mut state = state(tmp.path());
    let one = state.open_document(tmp.path().join("one.md")).expect("one");
    let two = state
        .open_document(tmp.path().join("two.markdown"))
        .expect("two");

    state.set_document_content(one, "# One\n\nEdited\n", Cursor::new(2, 5), 88.0);
    state.activate(two);

    assert_eq!(state.tabs().len(), 2);
    assert!(state.tabs().iter().any(|tab| tab.id == one && tab.dirty));
    assert!(state.tabs().iter().any(|tab| tab.id == two && !tab.dirty));

    let session = state.session();
    let session_path = tmp.path().join("restored-session.json");
    session.save(&session_path).expect("save session");
    let loaded = Session::load(&session_path);

    let mut restored = WorkspaceState::new(
        tmp.path().join("restored-session-2.json"),
        tmp.path().join("restored-recovery"),
    );
    let report = restored.restore_session(&loaded).expect("restore");

    assert_eq!(report.tabs_opened, 2);
    assert_eq!(restored.active_id(), Some(two));
    let recovered = restored.document(one).expect("restored one");
    assert_eq!(recovered.cursor, Cursor::new(2, 5));
    assert_eq!(recovered.scroll, 88.0);
}

#[test]
fn autosave_and_explicit_save_write_and_failures_keep_the_tab_dirty() {
    let tmp = workspace_fixture();
    let mut state = state(tmp.path());
    let id = state
        .open_document(tmp.path().join("one.md"))
        .expect("open");

    state.set_document_content(id, "# Explicit\n", Cursor::default(), 0.0);
    state.save(id).expect("explicit save");
    assert_eq!(
        fs::read_to_string(tmp.path().join("one.md")).expect("read"),
        "# Explicit\n"
    );

    state.set_document_content(id, "# Autosaved\n", Cursor::default(), 0.0);
    let outcomes = state.autosave_due(Instant::now(), Duration::ZERO);
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].saved);

    let read_only = tmp.path().join("read-only.md");
    fs::write(&read_only, "# Locked\n").expect("fixture");
    let mut permissions = fs::metadata(&read_only).expect("metadata").permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&read_only, permissions).expect("read-only");
    let locked = state.open_document(&read_only).expect("open locked");
    state.set_document_content(locked, "# Changed\n", Cursor::default(), 0.0);
    let outcome = state.save_outcome(locked);

    assert!(!outcome.saved);
    assert!(outcome.error.is_some());
    assert!(state.document(locked).expect("locked").is_dirty());
}

#[test]
fn external_changes_reload_conflict_or_mark_missing() {
    let tmp = workspace_fixture();
    let mut state = state(tmp.path());
    let clean = state
        .open_document(tmp.path().join("one.md"))
        .expect("clean");
    let dirty = state
        .open_document(tmp.path().join("two.markdown"))
        .expect("dirty");
    let deleted = state
        .open_document(tmp.path().join("three.mdown"))
        .expect("deleted");

    state.set_document_content(dirty, "# Mine\n", Cursor::default(), 0.0);
    fs::write(tmp.path().join("one.md"), "# External\n").expect("external clean");
    fs::write(tmp.path().join("two.markdown"), "# Theirs\n").expect("external dirty");
    fs::remove_file(tmp.path().join("three.mdown")).expect("delete");

    let changes = wait_for_changes(&mut state, 3);

    assert!(
        changes
            .iter()
            .any(|event| event.change == ExternalChange::Reloaded)
    );
    assert!(
        changes
            .iter()
            .any(|event| event.change == ExternalChange::Conflict)
    );
    assert!(
        changes
            .iter()
            .any(|event| event.change == ExternalChange::Missing)
    );
    assert_eq!(state.document(clean).expect("clean").text, "# External\n");
    assert!(state.document(dirty).expect("dirty").conflict.is_some());
    assert!(state.document(deleted).expect("deleted").is_missing());
}

#[test]
fn utf8_crlf_and_undecodable_documents_are_safe() {
    let tmp = tempfile::tempdir().expect("tempdir");
    fs::write(tmp.path().join("utf8.md"), "# Café\n\n日本語\n").expect("utf8");
    fs::write(tmp.path().join("crlf.md"), "one\r\ntwo\r\n").expect("crlf");
    fs::write(tmp.path().join("bad.md"), [0xFF, 0xFE, 0x00]).expect("bad");
    let mut state = state(tmp.path());

    let utf8 = state
        .open_document(tmp.path().join("utf8.md"))
        .expect("utf8");
    assert!(state.document(utf8).expect("utf8").text.contains("日本語"));

    let crlf = state
        .open_document(tmp.path().join("crlf.md"))
        .expect("crlf");
    let id = crlf;
    state.set_document_content(id, "one\ntwo\nthree\n", Cursor::default(), 0.0);
    state.save(id).expect("save crlf");
    assert_eq!(
        fs::read(tmp.path().join("crlf.md")).expect("read"),
        b"one\r\ntwo\r\nthree\r\n"
    );

    let error = state
        .open_document(tmp.path().join("bad.md"))
        .expect_err("undecodable");
    assert!(error.to_string().contains("decoded") || error.to_string().contains("UTF-8"));
    assert_eq!(
        fs::read(tmp.path().join("bad.md")).expect("read"),
        [0xFF, 0xFE, 0x00]
    );
}

#[test]
fn imports_markdown_and_html_and_failed_html_leaves_the_workspace_unchanged() {
    let tmp = workspace_fixture();
    let source = tempfile::tempdir().expect("source");
    let mut state = state(tmp.path());
    let root = state.root().expect("root").to_path_buf();
    let before = fs::read_dir(&root).expect("root").count();

    let markdown = source.path().join("imported.md");
    fs::write(&markdown, "# Imported\r\n").expect("markdown");
    let result = state
        .import_file(&markdown, &root)
        .expect("import markdown");
    assert_eq!(result.path, root.join("imported.md"));
    assert_eq!(fs::read(&result.path).expect("read"), b"# Imported\r\n");

    let html = source.path().join("page.html");
    fs::write(&html, "<h1>Page</h1><p>Hello <strong>world</strong></p>").expect("html");
    let result = state.import_file(&html, &root).expect("import html");
    assert!(result.converted_from_html);
    assert_eq!(result.path.extension().unwrap(), "md");
    let converted = fs::read_to_string(&result.path).expect("read converted");
    assert!(converted.contains("Page"));
    assert!(converted.contains("**world**"));

    let invalid = source.path().join("empty.html");
    fs::write(&invalid, "   \n").expect("invalid");
    let error = import::import(&invalid, &root, &root).expect_err("invalid html");
    assert!(matches!(error, ImportError::InvalidHtml(_)));
    assert_eq!(fs::read_dir(&root).expect("root").count(), before + 2);
}

#[test]
fn valid_front_matter_is_hidden_while_invalid_front_matter_stays_text() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let valid =
        "---\ntitle: Release notes\ndate: 2026-10-29\ntags: [rust, markdown]\n---\n# Body\n";
    let invalid = "---\ntitle: [unclosed\n---\n# Body\n";
    fs::write(tmp.path().join("valid.md"), valid).expect("valid");
    fs::write(tmp.path().join("invalid.md"), invalid).expect("invalid");
    let mut state = state(tmp.path());

    let id = state
        .open_document(tmp.path().join("valid.md"))
        .expect("valid");
    let document = state.document(id).expect("valid document");
    assert_eq!(document.display_title(), "Release notes");
    assert_eq!(document.front_matter().date(), Some("2026-10-29"));
    assert_eq!(document.front_matter().tags(), vec!["rust", "markdown"]);
    assert_eq!(document.front_matter().body(), "# Body\n");

    let id = state
        .open_document(tmp.path().join("invalid.md"))
        .expect("invalid");
    let document = state.document(id).expect("invalid document");
    assert!(document.front_matter().error().is_some());
    assert_eq!(document.front_matter().body(), invalid);
    assert_eq!(
        FrontMatter::parse(document.text.as_str()).body(),
        document.text
    );
}

#[test]
fn recovery_buffers_are_written_and_reopened_dirty() {
    let tmp = workspace_fixture();
    let mut state = state(tmp.path());
    let id = state
        .open_document(tmp.path().join("one.md"))
        .expect("open");
    state.set_document_content(id, "# Unsaved\n", Cursor::new(1, 3), 64.0);
    state.write_recovery().expect("write recovery");

    let records = state.pending_recovery();
    assert_eq!(records.len(), 1);
    let recovered = state.accept_recovery(records.into_iter().next().expect("record"));
    let document = state.document(recovered).expect("recovered");

    assert_eq!(document.text, "# Unsaved\n");
    assert!(document.is_dirty());
    assert_eq!(document.cursor, Cursor::new(1, 3));
    assert_eq!(document.scroll, 64.0);

    let store = RecoveryStore::new(tmp.path().join("standalone-recovery"));
    assert!(store.list().is_empty());
}

fn wait_for_changes(
    state: &mut WorkspaceState,
    minimum: usize,
) -> Vec<markmydown::workspace::state::ExternalChangeEvent> {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut changes = Vec::new();
    while Instant::now() < deadline {
        changes.extend(state.poll_external_changes());
        if changes.len() >= minimum {
            return changes;
        }
        thread::sleep(Duration::from_millis(50));
    }

    panic!("timed out waiting for {minimum} changes; got {changes:?}");
}
