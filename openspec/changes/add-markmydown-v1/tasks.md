# Tasks

## Planning assumptions

- Auto-update uses GitHub Releases for v1.
- macOS uses an in-window menu for v1; native menu integration is deferred.
- Linux support targets Ubuntu 22.04+ and Fedora with both Wayland and X11 smoke tests.
- The app opens file paths from the command line but does not expose a full CLI in v1.

## 1. Project foundation

- [x] 1.1 Initialize the Cargo workspace with the app crate and a vendored `iced-code-editor` crate; verify `cargo check` succeeds on macOS.
- [x] 1.2 Add and pin the v1 dependencies from `design.md`; verify `cargo build` succeeds and `cargo tree` shows one Iced version.
- [x] 1.3 Create the module skeleton from `design.md` (`app`, `workspace`, `editor`, `preview`, `export`, `theme`, `ui`, `config`); verify the project compiles with empty modules.
- [x] 1.4 Implement the Iced application shell with window creation, app state, message enum, update, and view; verify a native window opens with an empty layout.
- [x] 1.5 Add logging and platform config/data paths; verify a startup log entry is written to the expected directory.
- [x] 1.6 Add CI for macOS, Windows, and Ubuntu running fmt, clippy, and tests; verify the workflow passes on the scaffold.

## 2. Workspace and file safety

- [x] 2.1 Implement folder selection and the workspace tree model; verify opening a fixture folder lists `.md`, `.markdown`, `.mdown`, and `.txt` files and hides other files.
- [x] 2.2 Implement file and folder create, rename, delete, and move; verify each operation on disk and in the tree, including OS trash deletion.
- [x] 2.3 Implement tabs with dirty indicators and session restore; verify restarting restores open tabs, active tab, cursor, and scroll.
- [x] 2.4 Implement autosave, explicit save, and save-failure handling; verify a file is written after idle, Save writes immediately, and a read-only file shows an error without being marked saved.
- [x] 2.5 Implement atomic writes and mtime/hash tracking; verify no watcher feedback loop and no partial files after a simulated write failure.
- [x] 2.6 Implement the debounced file watcher, clean reload, conflict bar, and missing-file state; verify clean, dirty, and deleted external-change scenarios.
- [x] 2.7 Implement encoding and line-ending handling; verify UTF-8, CRLF, and an undecodable fixture without file corruption.
- [x] 2.8 Implement recovery buffers and crash recovery; verify killing the process with unsaved changes offers recovery on next launch.
- [x] 2.9 Implement Markdown and HTML import; verify Markdown import, HTML-to-Markdown conversion, and an invalid HTML fixture that leaves the workspace unchanged.
- [x] 2.10 Implement front matter parsing and metadata exposure; verify valid front matter is hidden from the body and invalid front matter is shown as text with a validation message.
- [x] 2.11 Add workspace and file-safety tests; verify `cargo test` covers the scenarios in `specs/document-workspace/spec.md`.
- [x] 2.12 Document the workspace model and file-safety behavior in `docs/workspace.md`; verify the documented scenarios match the tests.

## 3. Editor

- [ ] 3.1 Integrate the vendored editor with Markdown syntax highlighting; verify typing, selection, undo/redo, clipboard, and a 1 MB fixture remain responsive.
- [ ] 3.2 Implement formatting actions for bold, italic, heading, strikethrough, lists, quote, code, table, link, and image; verify each action and shortcut against the editor spec.
- [ ] 3.3 Implement smart list continuation, Tab/Shift+Tab indent, and paste-URL-over-selection; verify the three scenarios in the editor spec.
- [ ] 3.4 Implement find/replace with case sensitivity, regex, and replace-all; verify navigation, one-step undo, and invalid-regex handling.
- [ ] 3.5 Implement the outline and status bar; verify heading navigation, word/character/reading-time stats, and selection stats.
- [ ] 3.6 Patch the editor for focus mode: dim non-current text and keep the caret centered; verify the toggle and normal-mode restoration.
- [ ] 3.7 Implement editor-only, side-by-side, and preview-only layouts with resizable panes; verify each layout and pane persistence.
- [ ] 3.8 Spike IME and RTL input with Arabic and CJK fixtures; verify composition is not duplicated and document any limitations.
- [ ] 3.9 Add editor unit and UI tests; verify `cargo test` covers the scenarios in `specs/markdown-editor/spec.md`.
- [ ] 3.10 Document the editor adapter and vendored patch surface in `docs/editor.md`; verify the document explains how to update the vendored crate.

## 4. Preview

- [ ] 4.1 Implement the preview model from the comrak AST with block, inline, and source-range data; verify snapshot tests for representative fixtures.
- [ ] 4.2 Render headings, paragraphs, emphasis, lists, task lists, tables, quotes, code, images, links, and rules; verify visual fixtures for each block type.
- [ ] 4.3 Add syntect code highlighting and the copy action; verify supported, unknown, and empty code blocks.
- [ ] 4.4 Add block math rendering via `iced_math` or `latex-rust`; verify display equations, invalid equations, and inline math as literal text.
- [ ] 4.5 Add Mermaid rendering via `merman` to SVG; verify flowchart, invalid diagram, and oversized diagram scenarios.
- [ ] 4.6 Add relative image loading, missing-image placeholders, and external link handling; verify each scenario.
- [ ] 4.7 Add HTML sanitization; verify script tags, event handlers, and JavaScript URLs are neutralized while safe HTML renders.
- [ ] 4.8 Implement read mode with outline and reading progress; verify entering, outline navigation, and progress updates.
- [ ] 4.9 Implement two-way scroll synchronization; verify editor-to-preview, preview-to-editor, and a long-document fixture without visible jumping.
- [ ] 4.10 Add preview debounce, math/Mermaid caching, block virtualization, and the large-file threshold; verify a 1 MB document remains responsive and can be refreshed manually.
- [ ] 4.11 Add preview model and render snapshot tests; verify `cargo test` covers the scenarios in `specs/markdown-preview/spec.md`.
- [ ] 4.12 Document the preview model and extension points in `docs/preview.md`; verify the document matches the implemented block and inline types.

## 5. Export

- [ ] 5.1 Implement the theme token model and CSS generation; verify editor, preview, and export use the same tokens.
- [ ] 5.2 Implement Markdown export with save dialog and `.md` extension handling; verify export, cancel, and missing extension.
- [ ] 5.3 Implement standalone HTML export with embedded CSS, images, math, and Mermaid; verify the output opens offline and matches the active theme.
- [ ] 5.4 Implement PDF export with `pdfrs` and embedded math/Mermaid SVG; verify page size, margins, long documents, and diagram rendering.
- [ ] 5.5 Implement copy as HTML and plain text; verify paste into a rich-text editor and the plain-text fallback.
- [ ] 5.6 Implement print using the PDF layout; verify the system dialog and cancellation.
- [ ] 5.7 Implement export failure handling; verify unwritable and invalid destinations keep the document open.
- [ ] 5.8 Add export consistency snapshots and tests; verify preview, HTML, and PDF contain the same headings, lists, links, code, math, and diagrams.
- [ ] 5.9 Document export behavior and limitations in `docs/export.md`; verify the documented commands and scenarios.

## 6. Appearance, onboarding, and UI

- [ ] 6.1 Implement the theme registry with light, dark, system, and gallery themes; verify theme switching, system following, and persistence.
- [ ] 6.2 Implement theme preview on focus or hover and cancel-on-leave; verify the three scenarios in the appearance spec.
- [ ] 6.3 Implement editor and preview typography settings independently; verify each change and reset.
- [ ] 6.4 Implement settings persistence, corruption handling, and reset; verify restart, unreadable settings, and reset confirmation.
- [ ] 6.5 Implement the welcome document and in-app cheat sheet; verify first launch, offline rendering, open, and dismiss.
- [ ] 6.6 Implement quick open; verify fuzzy search and opening a document.
- [ ] 6.7 Implement notifications and error dialogs; verify success and failure states.
- [ ] 6.8 Add appearance and UI tests and snapshots; verify `cargo test` covers the scenarios in `specs/app-appearance/spec.md`.
- [ ] 6.9 Document theme tokens and UI conventions in `docs/theme.md`; verify the document matches the implemented tokens.

## 7. Desktop platform and packaging

- [ ] 7.1 Implement file associations, open-with, and command-line file paths; verify on macOS, Windows, and Linux.
- [ ] 7.2 Configure `cargo-packager` for macOS, Windows, and Linux installers; verify install and uninstall on each platform.
- [ ] 7.3 Implement update checks and installation via `self_update` and GitHub Releases; verify available, install, and offline scenarios.
- [ ] 7.4 Add macOS signing and notarization plus Windows signing to CI; verify signed artifacts pass platform checks.
- [ ] 7.5 Verify security and privacy behavior: no network on edit, no script execution, and update metadata only; record results.
- [ ] 7.6 Implement local diagnostics logs and the open-log-folder action; verify log creation, folder opening, and no document content in logs.
- [ ] 7.7 Add a cross-platform release workflow that builds and uploads installers; verify artifacts for all three platforms.

## 8. Integration and release readiness

- [ ] 8.1 Run an end-to-end smoke test: open a folder, edit, preview a table/math/Mermaid document, export HTML and PDF, restart, and recover a session; verify every step.
- [ ] 8.2 Benchmark a 1 MB document: open under 200 ms, keystroke under 16 ms, preview update under 80 ms, and export time recorded; fix regressions above these thresholds.
- [ ] 8.3 Audit keyboard navigation, IME, and RTL behavior; document gaps and verify the app remains usable without a mouse.
- [ ] 8.4 Write the README with install, build, and usage instructions; verify the documented commands work from a clean checkout.
- [ ] 8.5 Build release candidates for macOS, Windows, and Linux; verify version, signatures, file associations, and update checks.

## Workflow follow-up

- Archive the change after the project's review requirements are satisfied.
- Verify the archived specs and change status.
