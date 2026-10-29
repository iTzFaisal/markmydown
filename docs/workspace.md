# Workspace and file safety

`markmydown` treats a local folder as the workspace and plain files as the
source of truth. There is no database and no required network. This document
describes the workspace model, the file-safety rules, and the tests that cover
the behavior in
[`specs/document-workspace/spec.md`](../openspec/changes/add-markmydown-v1/specs/document-workspace/spec.md).

## Module map

| Module | Responsibility |
|---|---|
| `workspace::tree` | Scans a folder into supported documents and directories. |
| `workspace::document` | One open tab: text, file metadata, dirty state, conflicts, and saves. |
| `workspace::state` | Stateful orchestration: folder selection, tabs, saves, watcher, session, and recovery. |
| `workspace::ops` | Create, rename, move, and move-to-trash operations with path validation. |
| `workspace::atomic` | Atomic writes, fingerprints, and hashes. |
| `workspace::encoding` | UTF-8 decoding, BOM handling, and line-ending preservation. |
| `workspace::session` | Open tabs, active tab, cursor, and scroll persisted as JSON. |
| `workspace::recovery` | Unsaved buffers written to the app-data directory. |
| `workspace::watcher` | Debounced filesystem notifications. |
| `workspace::import` | Markdown copy and HTML-to-Markdown conversion. |
| `workspace::frontmatter` | YAML front matter parsing and metadata exposure. |
| `ui::sidebar` | Tree rendering and workspace actions. |
| `app` | Folder dialogs, tabs, action dialogs, conflict/missing bars, and recovery prompt. |

## Opening a folder

The user picks a folder with the native folder dialog. The application scans it
and remembers the folder in settings (`last_workspace` and
`recent_workspaces`), then restores it on the next launch when a session is not
available.

The tree:

- Includes `.md`, `.markdown`, `.mdown`, and `.txt` files.
- Includes subdirectories so an empty folder hierarchy remains visible.
- Hides every other file type, including images and HTML.
- Skips hidden entries, `.git`, `node_modules`, and `target`.
- Follows the standard ignore files through the `ignore` crate.
- Sorts directories before files, then by case-insensitive name.

An empty folder is a valid workspace. The sidebar and content area show
actions to create a document or choose another folder.

## Tabs and session restore

Every open document is a tab. A tab shows a dirty marker when its text differs
from the last saved text. Tabs also show `!` when the file is missing or has an
unresolved external-change conflict.

The session stores:

- the workspace root,
- the paths of open tabs, and
- the active tab, cursor line/column, and scroll offset for each tab.

The session is written atomically every few seconds and when tabs are opened,
closed, renamed, or moved. On launch, missing files are skipped and counted
rather than reopened as broken tabs. The active tab is restored when it still
exists; otherwise the first restored tab becomes active.

## Saving and autosave

- **Explicit Save** writes the active document immediately.
- **Save all** writes every dirty document.
- **Autosave** writes a dirty document after `autosave_ms` of no typing
  (default 1000 ms).
- Autosave skips documents with an unresolved conflict and documents whose
  file was deleted externally. Those require an explicit user decision.
- A failed write leaves the tab open and dirty. The error is shown in the
  document banner and the autosave status; the document is never marked saved.

Saving preserves the file's original line ending and BOM. A successful save
updates the stored content hash and mtime fingerprint and removes the
document's recovery buffer.

## Atomic writes and watcher feedback

Every write uses the same sequence:

1. Create a temporary file in the destination directory with a hidden,
   app-specific name.
2. Write the complete bytes and `fsync` the temporary file.
3. Copy the original permissions when the file already existed.
4. Rename the temporary file over the destination.
5. `fsync` the parent directory on Unix.

If any step fails, the temporary file is removed and the original file remains
byte-for-byte intact. There is no partial destination file.

A document stores a `FileFingerprint` after every open and save: length, optional
mtime, and a SHA-256 content hash. The watcher ignores temporary files and
ignored directories. When an event arrives, the app compares the fingerprint of
the file on disk with the fingerprint of the last app write. An event caused by
our own atomic write therefore produces no reload and no conflict, which
prevents a watcher feedback loop.

The watcher uses `notify` through `notify-debouncer-mini` with a 250 ms debounce.
Events are collapsed by path. Directory events, new files, and deletions refresh
the tree.

## External changes

When a watched file changes:

- **Clean tab:** the document reloads silently and the editor buffer is reset.
- **Dirty tab:** the document keeps local edits and records a conflict with the
  current disk fingerprint. A conflict bar offers:
  - **Reload disk** — discard local edits and reload.
  - **Keep mine** — suppress the conflict for this disk version. The next save
    overwrites the file.
  - **Compare** — show local and disk text side by side.
- **Deleted file:** the tab is marked missing. The bar offers **Save again**
  (recreate the file) and **Close tab**. The tab is not removed automatically.

The app never silently overwrites an external change. Autosave is disabled for
conflicted and missing documents.

## Encoding and line endings

- Documents are decoded as UTF-8 with an optional UTF-8 BOM.
- Internal text uses `\n`.
- The first detected line ending (`LF`, `CRLF`, or `CR`) is restored on save.
- UTF-16 and invalid UTF-8 are rejected with an error. The file is left
  unchanged; the application never guesses an encoding or rewrites the bytes.

## Recovery buffers

While a tab is dirty, a recovery record is written to the platform app-data
directory every few seconds. Each record contains the path, title, text, save
time, cursor, and scroll. Records are keyed by a hash of the document path so
tabs cannot overwrite each other.

On launch, the application lists recovery records and shows a prompt to recover
or discard each one. Recovered content opens as a dirty tab and keeps its
original path, cursor, and scroll. A clean save or explicit close removes the
record. Corrupt records are skipped without blocking startup.

## Import

- Markdown files are copied byte-for-byte into the selected workspace folder,
  preserving their line endings.
- A Markdown file already inside the workspace is opened without creating a
  duplicate.
- HTML and HTM files are decoded as UTF-8, converted with `html2md`, and written
  only after a successful conversion.
- Name collisions receive `-1`, `-2`, and so on.
- Invalid or empty HTML reports an error and leaves the workspace unchanged.

## Front matter

YAML front matter is recognized only when the document starts with a `---`
line and is closed by `---` or `...`.

- Valid front matter is removed from the preview body and exposed through
  `FrontMatter::title()`, `date()`, `tags()`, and `metadata()`. Export can use
  the same metadata.
- Invalid YAML or an unterminated block is treated as ordinary text, with a
  validation message in the document banner. The body is not truncated, so the
  user never loses content.

## Test coverage

The workspace scenarios are covered by unit tests in the modules and by
`crates/markmydown/tests/workspace_spec.rs`. The mapping is:

| Spec scenario | Test |
|---|---|
| Open a folder | `opens_a_folder_with_supported_documents_and_an_empty_state` |
| Empty folder | `opens_a_folder_with_supported_documents_and_an_empty_state` |
| Unsupported files | `opens_a_folder_with_supported_documents_and_an_empty_state` |
| Create a document | `manages_files_and_folders_on_disk_and_in_the_tree` |
| Rename a document | `manages_files_and_folders_on_disk_and_in_the_tree` |
| Delete a document | `manages_files_and_folders_on_disk_and_in_the_tree` |
| Move a document | `manages_files_and_folders_on_disk_and_in_the_tree` |
| Multiple tabs | `tabs_show_dirty_state_and_restore_the_whole_session` |
| Restore a session | `tabs_show_dirty_state_and_restore_the_whole_session` |
| Recover after a crash | `recovery_buffers_are_written_and_reopened_dirty` |
| Autosave | `autosave_and_explicit_save_write_and_failures_keep_the_tab_dirty` |
| Explicit save | `autosave_and_explicit_save_write_and_failures_keep_the_tab_dirty` |
| Save failure | `autosave_and_explicit_save_write_and_failures_keep_the_tab_dirty` |
| Clean file changed on disk | `external_changes_reload_conflict_or_mark_missing` |
| Dirty file changed on disk | `external_changes_reload_conflict_or_mark_missing` |
| Open file deleted on disk | `external_changes_reload_conflict_or_mark_missing` |
| UTF-8 document | `utf8_crlf_and_undecodable_documents_are_safe` |
| CRLF line endings | `utf8_crlf_and_undecodable_documents_are_safe` |
| Undecodable document | `utf8_crlf_and_undecodable_documents_are_safe` |
| Import Markdown | `imports_markdown_and_html_and_failed_html_leaves_the_workspace_unchanged` |
| Import HTML | `imports_markdown_and_html_and_failed_html_leaves_the_workspace_unchanged` |
| Import failure | `imports_markdown_and_html_and_failed_html_leaves_the_workspace_unchanged` |
| Valid front matter | `valid_front_matter_is_hidden_while_invalid_front_matter_stays_text` |
| Invalid front matter | `valid_front_matter_is_hidden_while_invalid_front_matter_stays_text` |
| Remember folder for next launch | `open_folder_is_remembered_for_the_next_launch` |

Atomic-write, permissions, hash, and failure-injection cases are covered in
`workspace::atomic::tests`; watcher filtering is covered in
`workspace::watcher::tests`.

## Known limits in this milestone

- The preview pane is a placeholder until the preview milestone; the workspace
  model already exposes front matter metadata for it.
- The editor milestone adds formatting, search/replace, outline, status,
  focus mode, and editor/preview layouts; see
  [`docs/editor.md`](editor.md).
- Read mode currently reuses the preview-only layout until reading typography
  and scroll synchronization arrive with the preview milestone.
- The watcher is best-effort. A file changed while the app is not running is
  detected when the watcher starts or when the document is next saved.
