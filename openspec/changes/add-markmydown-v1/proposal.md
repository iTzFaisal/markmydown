# Proposal

## Why

Markdown writers need a fast, offline, file-first editor that feels native and gets out of the way. Existing options are either web/Electron apps with heavy runtimes and cloud assumptions (StackEdit) or note-taking suites with proprietary storage and feature bloat. markmydown is a pure-Rust, native Markdown workbench: it opens a local folder, edits plain `.md` files, renders a beautiful preview, and exports publishable HTML/PDF. No webview, no accounts, no lock-in.

Why now: Rust's native GUI and Markdown ecosystem is finally credible. Iced 0.14, `iced-code-editor`, `comrak`, `iced_math`, `merman` (used by Zed), and `pdfrs` make a cross-platform, pure-Rust app feasible without embedding a browser.

## What Changes

- Introduce a new cross-platform desktop app for Windows, macOS, and Linux, built in Rust with Iced 0.14 and no webview.
- Add a local-folder workspace: file tree, folders, create/rename/delete/move, tabs, autosave, session restore, recent folders, and external-change detection with conflict handling.
- Add a source Markdown editor: syntax highlighting, formatting toolbar, fixed keyboard shortcuts, smart lists, find/replace, outline, status bar, focus mode, and editor/preview/read layout modes.
- Add a native Markdown preview: GFM, tables, task lists, footnotes, definition lists, abbreviations, sub/sup, mark, emoji, SmartyPants, code highlighting, block math, Mermaid diagrams, images, links, and HTML sanitization. Scroll sync between editor and preview.
- Add a dedicated read mode with reading typography, outline, and reading progress.
- Add document I/O: import Markdown and HTML, export Markdown, standalone HTML, and PDF, copy as HTML/rich text, print, and image paste/drop into a local `assets/` folder.
- Add appearance and preferences: light/dark/system theme, font family/size/line height/text width, editor and preview font independence, and locally persisted settings.
- Add desktop distribution: signed/notarized installers for macOS, Windows, and Linux; open `.md` files from the OS; offline-first operation; auto-update.
- Explicit non-goals for this change: cloud sync, publishing integrations, OAuth accounts, real-time collaboration, comments, multiple workspaces, version history, mobile, plugin system, AI assistance, backlinks/graph, Pandoc export, custom export templates, and gamification.

**BREAKING**: None. This is a greenfield application.

## Capabilities

### New Capabilities

- `document-workspace`: local folder selection, file tree, tabs, autosave, session restore, external-change handling, front matter, and document import.
- `markdown-editor`: the source editing surface, Markdown highlighting, formatting actions, shortcuts, search/replace, outline, focus mode, and layout modes.
- `markdown-preview`: native rendering of GFM and extensions, code highlighting, block math, Mermaid, images, links, sanitization, read mode, and scroll sync.
- `document-export`: Markdown/HTML/PDF export, copy as HTML, print, and theme-consistent output.
- `app-appearance`: themes, typography, settings persistence, welcome content, and the Markdown cheat sheet.
- `desktop-platform`: native cross-platform packaging, file associations/open-with, offline operation, and updates.

### Modified Capabilities

- None. The project has no existing specs.

## Impact

- New Rust workspace with a native Iced application. No existing code is modified.
- New dependencies: `iced`, `iced-code-editor`, `comrak`, `syntect`, `iced_math`, `merman`, `pdfrs`, `rfd`, `notify`, `ignore`, `encoding_rs`, `serde`, `directories`, `cargo-packager`, and `self_update`.
- New platform deliverables: macOS `.app`/`.dmg`, Windows MSI/NSIS, Linux AppImage/deb.
- No server, network service, database, or cloud account is required for core use.
- Risk areas to validate in the first implementation spike: `iced-code-editor` maturity and IME behavior, Iced accessibility support, PDF fidelity with embedded math/Mermaid SVG, scroll-sync accuracy, and cross-platform packaging/signing.
