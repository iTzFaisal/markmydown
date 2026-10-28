# Design

## Context

See `proposal.md` for motivation and `specs/` for the behavior contract. This is a greenfield application: no source, tests, or existing specs. The design must satisfy three hard constraints:

- Pure Rust, native desktop UI with no webview or browser runtime.
- Windows 10/11, macOS 12+, and Linux from one codebase.
- Local plain-text files are the source of truth; no server, database, or required network.

The app is large enough to need explicit module boundaries: a stateful editor, a derived preview model, a file-safety layer, a theme system shared with export, and a cross-platform packaging pipeline.

## Goals / Non-Goals

**Goals:**

- A native Iced application with a clear Elm-style architecture and testable domain modules.
- One Markdown render model shared by the preview, HTML export, and PDF export.
- File safety that makes external edits, crashes, and autosave trustworthy.
- A theme token system that keeps editor, preview, and exported output visually consistent.
- A repeatable packaging and update pipeline for all three operating systems.

**Non-Goals:**

- Cloud sync, publishing, accounts, collaboration, comments, or multiple workspaces.
- Version history, plugins, AI assistance, mobile, or backlinks.
- Inline math typesetting in v1; block math only.
- Pixel-identical PDF output versus preview; content parity is required, not pixel parity.
- Full screen-reader support on Iced 0.14, which has no AccessKit integration. Keyboard-first operation is required; this is a known platform gap.

## Decisions

### 1. Native Rust with Iced 0.14

Iced provides a native, GPU-accelerated (or software) renderer, an Elm-style update loop, `pane_grid` for the split view, and animation primitives. It is the only pure-Rust toolkit with an editor widget that matches the latest framework version.

Alternatives considered:

- **egui**: more mature, and `egui_commonmark_extended` already renders math and Mermaid, but the editor widgets are basic and the preview fork is pinned to an older egui.
- **GPUI**: Zed-grade rendering, but the published crate is 0.2.2, the API is unstable, and there are no reusable editor or preview widgets.
- **Slint**: polished, but no code editor or Markdown preview; licensing is GPL, royalty-free, or commercial.
- **Tauri**: excluded by the no-webview requirement.

Consequence: Iced is pre-1.0. Pin the version, isolate UI code behind modules, and expect selective upgrades rather than blind updates.

### 2. Vendor and patch `iced-code-editor`

`iced-code-editor` already provides multi-cursor editing, folding, search and replace, a command palette, sticky scroll, line numbers, auto-indent, auto-closing brackets, and optional vim mode. The focus-mode requirement (centered caret, dimmed non-current text) and Markdown-specific input rules need internal rendering access, so the crate is vendored as a workspace crate under its MIT license and patched behind a small adapter.

Patch surface to keep intentionally small:

- A per-line opacity hook for focus mode.
- A caret-centering scroll policy for typewriter behavior.
- Markdown input rules and decoration hooks.

Alternatives considered:

- **Unmodified dependency**: cannot meet focus mode or Markdown decorations.
- **Build a custom editor**: a multi-month project before the first useful release.
- **`egui_code_editor`**: far fewer editing capabilities.

Consequence: a maintenance burden, but a bounded one. Upstream changes are merged selectively, and the adapter keeps the rest of the app independent of the editor internals.

### 3. Custom preview model, not the built-in Markdown widget

`iced::widget::markdown` is a useful reference for headings, lists, tables, images, and code, but it has no source ranges, no math, no Mermaid, and no extension points for custom blocks. The app needs those for scroll sync, block math, diagrams, and export parity.

The preview pipeline is:

```text
Markdown source
  -> comrak AST (GFM, front matter, math, source positions)
  -> preview model (blocks, inline spans, source ranges)
  -> Iced widgets
```

The model separates block types (heading, paragraph, list, table, code, quote, math, Mermaid, image, rule) from inline types (text, emphasis, strong, code, link, image, break). Each block carries its source range for scroll synchronization. Rendered math and Mermaid SVGs are cached by content hash.

Alternatives considered:

- **Built-in Markdown widget**: fastest to start, but cannot carry the differentiating features.
- **Fork the built-in widget**: less control than a purpose-built model and still lacks source ranges.
- **`frostmark`**: promising, but early and without math or Mermaid.

Consequence: more code, but the preview model is the product's visual core and the source of truth for export.

### 4. One Markdown pipeline: `comrak`, `syntect`, `merman`, `iced_math`

- `comrak` parses GFM, front matter, footnotes, definition lists, tables, and dollar math, and exposes source positions.
- `syntect` highlights fenced code in both the editor and preview.
- `merman` renders Mermaid to SVG headlessly; it is the backend Zed uses.
- `iced_math` renders block equations natively; `latex-rust` is the fallback for SVG output.

Alternatives considered:

- **`pulldown-cmark`**: fast and used by Iced's built-in widget, but lacks the math extension and source-position convenience the preview model needs.
- **`mermaid-rs-renderer`**: viable, but less proven than `merman`.
- **Typst for whole-document rendering**: beautiful typesetting, but page-based and loses interactive read mode and scroll sync.

Consequence: inline math remains literal text in v1. This is an explicit scope boundary, not an implementation accident.

### 5. Export pipeline: shared model, `pdfrs` for PDF

- HTML export uses the same preview model and theme tokens, rendered to semantic HTML with a `minijinja` template and embedded CSS.
- PDF export uses `pdfrs` with block math and Mermaid pre-rendered to SVG and embedded. Page size and margins come from settings.
- Print uses the same layout as PDF.

Alternatives considered:

- **Typst/Mitex for PDF**: stronger typesetting, but a second rendering pipeline that can drift from the preview. Keep it as a fallback if `pdfrs` cannot meet fidelity.
- **System print dialog only**: not enough control over pagination.
- **`mdxport`**: oriented around a standalone app; less control as a library.

Consequence: PDF is content-equivalent, not pixel-identical. The export tests compare structure and presence of math, diagrams, and code, not exact line breaks.

### 6. File safety model

- Atomic writes: write a temporary file in the destination directory, fsync, then rename over the original.
- Track modification time and a content hash after every write to distinguish the app's writes from external writes.
- Watch the workspace with a debounced watcher; ignore `.git`, `node_modules`, and hidden directories.
- On external change: reload silently if clean; show a non-modal conflict bar if dirty; mark missing files instead of deleting tabs.
- Write recovery buffers to the app data directory every few seconds for unsaved documents.
- Preserve line endings; decode UTF-8 by default and report undecodable files rather than guessing.

Alternatives considered:

- **Naive write**: risks data loss and watcher feedback loops.
- **Exclusive file locks**: not portable or friendly to git and cloud-drive workflows.
- **No watcher**: leaves stale content on screen.

Consequence: more moving parts, but file trust is the product's foundation.

### 7. Elm-style state, module boundaries, and theme tokens

The app follows Iced's Elm architecture:

- `App` owns workspace, tabs, settings, theme, and UI state.
- `Document` owns one tab's path, dirty state, editor content, cursor, scroll, and derived preview model.
- `update(Message)` returns `Task<Message>` for async file I/O, export, and update checks.
- `Subscription` carries debounced watcher events and update notifications.
- The editor owns its text buffer and undo history; the app does not mirror document text into global state on every keystroke.

Module layout:

```text
src/
  main.rs
  app.rs          App state, Message, update, view
  workspace/      tree, tabs, session, watcher, recovery
  editor/         adapter over the vendored editor, Markdown behaviors
  preview/        comrak parser, model, renderer, math, mermaid, cache
  export/         HTML template, PDF, copy, print
  theme/          token model, theme registry, export CSS
  ui/             command palette, dialogs, sidebar, status bar
  config/         settings, paths, versioned schema
```

Theme tokens are the single source of truth. One `ThemeTokens` value maps to the Iced theme, the editor color theme, preview styles, and export CSS. A theme change updates all four from the same data.

Alternatives considered:

- **Global mutable store**: harder to test and easier to create state drift.
- **Per-feature theme values**: visual inconsistency between editor, preview, and export.

Consequence: more explicit plumbing, but predictable state and consistent output.

### 8. Packaging, updates, and quality gates

- `cargo-packager` produces macOS `.app`/`.dmg`, Windows MSI/NSIS, and Linux AppImage/`.deb`, including file associations.
- `self_update` checks a signed release feed and installs updates. GitHub Releases is the v1 feed.
- CI runs the test suite on macOS, Windows, and Ubuntu, builds installers, and signs/notarizes macOS and Windows artifacts.
- Tests: unit tests for parsing, file I/O, and theme mapping; `insta` snapshots for preview models and HTML export; Iced tester for critical UI flows; a fixture corpus for GFM, math, Mermaid, front matter, Unicode, and long documents.
- Performance: debounce preview parsing, cache math and Mermaid by hash, virtualize long previews by block, and pause live preview above a size threshold with a manual refresh action.

Alternatives considered:

- **Per-platform packaging scripts**: more maintenance and more room for drift.
- **No auto-update**: a worse desktop experience and slower security fixes.

Consequence: signing and notarization are real work; start them early rather than at release time.

## Architecture

```text
+------------------------------------------------------------------+
|                        Iced app (Elm)                            |
|  state: workspace, tabs, settings, theme, ui                     |
+----------------+---------------------------+---------------------+
| update(Message) | view()                   | subscriptions       |
| file events     | sidebar / tabs / editor   | debounced watcher   |
| editor actions  | preview / status bar      | update checks       |
| export requests | overlays: palette, dialogs|                     |
+--------+--------+-------------+-------------+---------------------+
         |                      |
   Task::perform          preview model
   (async I/O)            (comrak AST)
         |                      |
   +-----v------+        +------v-------+
   | workspace  |        | Iced widgets |
   | fs, docs   |        | text, table, |
   +-----+------+        | code, math,  |
         |               | mermaid      |
   +-----v------+        +--------------+
   | vendored   |
   | editor     |
   +------------+
```

## Risks / Trade-offs

| Risk | Mitigation |
|---|---|
| `iced-code-editor` API churn or missing hooks | Vendor and patch it; keep the adapter small; merge upstream selectively |
| No AccessKit in Iced | Keyboard-first navigation; document the screen-reader gap; revisit the GUI choice if accessibility becomes a hard requirement |
| IME/RTL behavior in a canvas editor | Spike Arabic and CJK input early; patch the editor if composition is duplicated |
| `pdfrs` maturity | Embed SVG for math and Mermaid; test long documents; keep Typst/Mitex as a fallback |
| Scroll-sync drift | Source ranges plus measured block heights; accept approximate sync on very long documents |
| Preview cost on large files | Debounce, cache, block virtualization, and a manual-refresh threshold |
| Watcher storms from git or cloud sync | Debounce, ignore patterns, and hash-based change detection |
| Theme drift across editor, preview, and export | One token model; snapshot tests for each surface |
| Cross-platform signing and notarization | Build signed artifacts in CI from the first milestone |

## Migration Plan

Not applicable; this is a greenfield application. Deployment is through platform installers. Rolling back means uninstalling the app; user documents are untouched. Settings live in the platform app-data directory with a schema version so future settings migrations are possible.

## Open Questions

- Auto-update feed: GitHub Releases for v1, or a self-hosted feed later?
- macOS menu bar: in-window menu for v1, or native menus via `muda`?
- Linux baseline: which desktop environments and Wayland/X11 combinations are officially supported?
- Should the app expose a CLI beyond opening file paths, or remain GUI-only?
