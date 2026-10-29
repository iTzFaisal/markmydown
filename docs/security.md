# Security and privacy audit

This document records the v1 audit of the security and privacy requirements in
[`specs/desktop-platform/spec.md`](../openspec/changes/add-markmydown-v1/specs/desktop-platform/spec.md).

Audit date: 2026-10-29

## Architecture

markmydown is a native Rust/Iced application. It does not embed a browser,
webview, JavaScript runtime, or remote content loader. Markdown is parsed into a
Rust data model and rendered with native widgets. HTML export writes a static
file; it is never executed by the application.

## No network transmission while editing

Editing and preview do not make network requests.

- The workspace reads and writes local files only.
- The preview resolves relative images from the document folder. `http://`,
  `https://`, `data:`, and protocol-relative image URLs are deliberately shown
  as placeholders instead of being fetched.
- The editor, autosave, recovery, session, and watcher modules use local
  filesystem APIs.
- There is no telemetry, analytics, account, sync, or cloud storage code.
- External links open in the system browser only when the user activates them.
  The application never follows them on its own.
- The only network-capable subsystem is the update checker, described below.

Verification: code audit of `workspace/`, `preview/`, `editor/`, `export/`,
`config/`, and `app.rs`; no HTTP client is used outside `config::updates` and
`self_update`. The preview image tests cover remote-image rejection, and the
workspace tests perform all edits offline.

## Document scripts are not executed

- Fenced and inline code is displayed as text.
- Raw HTML blocks and inline HTML pass through `preview::sanitize` before they
  reach the model. The sanitizer uses `ammonia` with an allow-list of safe
  formatting tags; scripts, event handlers, `iframe`, `object`, `embed`, and
  unsafe URL schemes are removed.
- Links with `javascript:`, `vbscript:`, and `data:` targets are neutralized in
  both the preview and HTML export.
- Mermaid and math renderers consume source text through their own SVG
  generators; they do not evaluate user-provided scripts.
- The application has no JavaScript engine.

Verification:
`cargo test -p markmydown --test preview_spec unsafe_html_is_neutralized_while_safe_html_renders`
and the export HTML tests for unsafe links. These tests pass on the audit
commit.

## Update metadata

Update checks are explicit user actions from **Settings → Updates**. A check:

1. Calls the GitHub Releases API for the project's public repository.
2. Requests release metadata (version, name, assets) only.
3. Does not download the artifact until the user chooses **Install and
   restart**.
4. Sets `.show_output(false)` and `.no_confirm(true)` so the update never blocks
   on terminal input.

An offline or failed check reports the error without blocking startup or
editing. The app does not check automatically on launch.

Installation uses `self_update` with the `checksums` feature. GitHub's
per-asset digest and release `SHA256SUMS` metadata are used for verification
when available. The release workflow builds artifacts from the tagged source
and uploads them to GitHub Releases.

Verification: `config::updates::check` and `install_and_restart` are the only
network paths; `config::updates::tests::updater_configuration_is_valid_without_network_access`
builds the updater without making a request. The manifest contains no update
token or private feed.

## Logs

`config::logging` writes a local append-only file under the platform data
directory. Records contain a timestamp, level, target, and message built by the
application.

- Errors log paths, operation names, and error descriptions.
- Document text, preview text, selection text, clipboard content, and settings
  values are never passed to the logger.
- The startup record contains version, OS, architecture, and app-data paths.
- The log can be revealed with **Settings → Diagnostics → Open log folder**.

Verification:
`config::logging::tests::records_do_not_include_document_content` writes a
record containing only a path and byte count, then asserts that a private
document body is absent from the file. `startup_record_is_written_to_the_expected_directory`
covers log creation.

## Recovery and local data

- Recovery buffers and session state are stored under the platform data
  directory, not in the workspace.
- Atomic writes prevent partial files.
- Unreadable settings are preserved as `settings.corrupt-<pid>.json` for
  diagnosis; the application starts with defaults.
- User documents are never deleted by uninstall logic. Delete operations move
  files to the operating-system trash.

## Findings and residual risks

| Finding | Severity | Status |
|---|---|---|
| Update installs replace the executable, which can invalidate a macOS app-bundle code signature | Medium | Documented limitation; signed-notarized release builds must be re-signed after an update, or distributed through the installer instead. |
| External links open in the system browser on user action | Low | Expected behavior; the URL scheme is limited to http, https, and mailto. |
| Remote images are shown as placeholders rather than fetched | Low | Deliberate offline/privacy choice. |
| Iced 0.14 has no AccessKit integration | Medium (accessibility, not security) | Keyboard-first operation and documented screen-reader gap. |
| No automated malware scanning of update artifacts beyond SHA-256 verification | Low | Artifacts come from the project's GitHub release feed and are digest-verified. |

No document content is transmitted during editing, preview, export, printing,
or copy. No document script executes. Update checks request release metadata
only and run on demand.
