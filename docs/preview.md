# Preview

The preview is the visual core of markmydown. It is also the source of truth for
HTML and PDF export: all three surfaces consume the same `PreviewDocument`, so a
block that appears in the preview appears in an export.

## Pipeline

```text
Markdown source
  -> comrak AST (GFM, front matter, math, source positions)
  -> PreviewDocument (blocks, inlines, source ranges)
  -> Iced widgets (screen), semantic HTML (export), pdfrs (PDF)
```

The parser lives in `src/preview/parser.rs`, the model in
`src/preview/model.rs`, and the Iced renderer in `src/preview/render.rs`. The
model has no Iced dependency, which keeps it snapshot-testable and reusable by
the export modules.

Front matter is removed before parsing and its line count is added back to every
source range, so scroll synchronization continues to point at the original file.
Malformed front matter stays in the body and is reported by the workspace
banner.

## Document model

`PreviewDocument` contains:

- `blocks`: block-level elements in source order.
- `footnotes`: footnote definitions, rendered after the main body.
- `line_offset`: number of source lines hidden by valid front matter.

Every block carries an inclusive `SourceRange` (`start_line`, `end_line`) used
for scroll synchronization and outline navigation.

Block types:

| Block | Notes |
|---|---|
| `Heading` | Level and inline spans |
| `Paragraph` | Inline spans, including images |
| `List` | Ordered or unordered, start number, tight/loose, task items |
| `DefinitionList` | Term plus one or more definitions |
| `Code` | Optional language, raw code |
| `Quote` | Nested blocks, including alerts and multiline quotes |
| `Table` | Column alignments, header cells, body rows |
| `Rule` | Thematic break |
| `Math` | Display math source |
| `Mermaid` | Mermaid diagram source |
| `Html` | Sanitized block HTML |

Inline types:

`Text`, `Emphasis`, `Strong`, `Strikethrough`, `Highlight`, `Underline`,
`Superscript`, `Subscript`, `Code`, `Link`, `Image`, `SoftBreak`, `Break`,
`FootnoteReference`, `Math`, and sanitized inline `Html`.

Inline math remains literal text by design. Only `$$...$$` display math is
typeset in v1.

## Extensions

The parser enables the full v1 extension set in comrak:

- GFM tables, task lists, strikethrough, and autolinks.
- Footnotes and description (definition) lists.
- Abbreviations where comrak exposes them, plus superscript, subscript,
  highlight, underline, and spoiler text.
- Emoji shortcodes.
- Smart punctuation (SmartyPants).
- Dollar math and fenced `math`/`latex` blocks.
- Alerts and callouts, mapped onto quote blocks.

## Rendering

`render::view` is a pure function of the model, theme tokens, preview
typography, render caches, and the current viewport. It returns a scrollable
column wrapped in a maximum-width reading container.

Highlights:

- Headings use a scale from the preview font size; body text uses the configured
  family, size, and line height.
- Links use the theme link color and send `Message::LinkClicked`; the app opens
  HTTP, HTTPS, and mailto URLs in the system browser and ignores unsafe schemes.
- Code blocks use syntect with a light or dark syntect theme chosen from the
  active markmydown theme, and include a Copy action.
- Tables use the model's alignments for each cell.
- Quotes and code blocks use theme surfaces and borders.
- Images are resolved relative to the document directory. A missing image shows
  its alt text and a placeholder instead of breaking the layout.
- Display math and Mermaid diagrams render to SVG. Invalid sources show the raw
  source with an error indicator.

## Math and Mermaid

`src/preview/math.rs` renders LaTeX to SVG with `iced_math` using the preview
font size and theme text color. `src/preview/mermaid.rs` renders Mermaid source
with `merman`. Both return typed errors rather than panicking.

Rendered output is cached in `RenderCaches` by a content hash that includes the
source, font size, and color where relevant. The caches are bounded (128 math
entries, 64 Mermaid entries) and evict the oldest entry. Because keys include
presentation parameters, a theme change naturally produces new entries.

SVG widgets use `ContentFit::Contain`, so a diagram wider than the preview
scales down instead of being clipped.

## HTML sanitization

All author HTML passes through `src/preview/sanitize.rs` before it reaches the
model. The sanitizer uses ammonia with a conservative allow-list:

- Safe presentational tags such as `kbd`, `br`, `mark`, `sub`, `sup`, `del`,
  `ins`, `small`, `span`, `p`, `div`, tables, and code survive.
- Script and style elements are removed together with their contents.
- Event handlers such as `onclick` are removed.
- URL schemes are restricted to HTTP, HTTPS, and mailto. `javascript:` and
  `data:` links are neutralized.

The renderer treats safe inline HTML as styled spans; block HTML is rendered as
text with safe line breaks and formatting hints.

## Read mode

Read mode (`LayoutMode::Read`) hides the toolbar, tab bar, formatting toolbar,
sidebar, and editor. It shows:

- The rendered document, filling the content area.
- An outline built from the document headings; selecting one scrolls the
  preview to the corresponding block.
- Reading progress in the header and status bar, derived from the preview
  viewport's scroll offset and content height.
- An Edit button that returns to side-by-side mode.

The outline can be toggled from the read-mode header. Reading progress is
computed by `preview::sync::reading_progress` and clamped to 0–100%.

## Scroll synchronization

`src/preview/sync.rs` maps source lines to blocks and scroll fractions:

- `block_for_line` finds the block containing a line, falling back to the
  closest preceding block.
- `preview_progress_for_line` converts an editor line to a normalized preview
  position.
- `editor_line_for_preview_progress` converts preview progress back to a source
  line.

In side-by-side mode the app listens to editor scrolls and preview scrolls.
Programmatic scrolls are briefly ignored on the receiving side, which prevents
feedback loops. The mapping is monotonic, so a long document never moves the
other pane backwards while the user scrolls forward.

The editor exposes its first visible logical line and line height through the
adapter; the preview uses those values as the synchronization anchor.

## Debounce, caching, and large documents

Editing does not reparse on every keystroke. The app marks the document preview
stale, records the edit time, and refreshes after
`settings.preview_debounce_ms` (default 80 ms). Cursor movement and pure scrolls
never trigger a reparse.

Documents with more blocks than `VIRTUALIZE_AFTER` (80) render only the blocks
around the current viewport:

- `block_window` estimates each block's height from the preview typography and
  returns the visible range plus top and bottom spacers.
- Overscan keeps eight blocks above and below the viewport so small scrolls do
  not rebuild the column on every event.
- Short documents always render every block, which keeps exact layout for
  ordinary notes.

Block heights are estimates, not measurements. On very long documents the
scrollbar and synchronisation are approximate, but the rendered widget count
stays bounded and the document remains responsive.

Documents larger than `settings.large_file_threshold` (default 1,000,000 bytes)
do not refresh the preview automatically. A banner offers **Refresh preview**,
which reparses on demand while keeping the editor responsive.

## Tests and fixtures

`tests/preview_spec.rs` covers the behavior contract in
`openspec/changes/add-markmydown-v1/specs/markdown-preview/spec.md`:

- Model and HTML snapshots for every block family
  (`tests/snapshots/preview_spec__*.snap`).
- GFM blocks, extended inline syntax, footnotes, and definition lists.
- Code highlighting for supported, unknown, and empty languages.
- Display math, invalid math, and literal inline math.
- Mermaid flowcharts, invalid sources, and oversized diagrams.
- Relative and missing images, external links, and sanitization.
- Monotonic scroll synchronization in both directions.
- Read-mode progress and outline mapping.
- A 1 MB parse and virtualized-window test.

Update preview snapshots intentionally with:

```bash
cd crates/markmydown
INSTA_UPDATE=always cargo test --test preview_spec
cargo test --test preview_spec
```

## Extension points

- Add a block type: extend `Block` and `Inline`, convert it in `parser.rs`, then
  render it in `render.rs` and in the export renderers.
- Add an inline decoration: add the variant to `Inline`, map the comrak node,
  and add a branch in `push_inlines`.
- Change theme integration: update `PreviewStyles` and the token usage in
  `render.rs`; exported HTML reads the same tokens through `theme::css`.
- Change caching: extend `RenderCaches` and `cache_key` so every presentation
  parameter that affects output participates in the key.

## Known limitations

- Inline math is literal text in v1.
- Virtualized height estimates can drift on extremely long documents; scroll
  mapping is intentionally approximate rather than pixel-perfect.
- Safe HTML is rendered as styled text, not as a full HTML layout engine.
- Math and Mermaid rendering are synchronous; very large diagrams can still
  cost one frame on first render before the cache is warm.
