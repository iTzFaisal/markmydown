# Export, copy, and print

Export turns the current document into portable Markdown, a standalone HTML
file, or a paginated PDF. Copy and print reuse the same preview model, so the
content shown on screen is the content that leaves the application. This
document describes the implementation and the tests that cover
[`specs/document-export/spec.md`](../openspec/changes/add-markmydown-v1/specs/document-export/spec.md).

## Module map

| Module | Responsibility |
|---|---|
| `export` | `ExportFormat`, `ExportRequest`, save-dialog choice handling, and the shared error type. |
| `export::markdown` | Writes the Markdown source unchanged and normalizes a missing `.md` extension. |
| `export::html` | Renders the preview model to semantic HTML, embeds the stylesheet, local images, math SVG, and Mermaid SVG. |
| `export::pdf` | Maps the preview model to `pdfrs` elements, applies page size and margins, and embeds rendered diagrams. |
| `export::clipboard` | Builds an HTML fragment plus a plain-text fallback and writes it to the system clipboard. |
| `export::print` | Generates the same PDF layout in a temporary file and hands it to the operating system print flow. |
| `theme::css` | Generates the exported stylesheet from the active `ThemeTokens` and preview `Typography`. |
| `app` | Toolbar actions, native save dialogs, export status, and the retry dialog for failures. |

## Actions

The toolbar exposes:

- **Export .md** — save the source as Markdown.
- **Export HTML** — save a standalone HTML document.
- **Export PDF** — save a paginated PDF.
- **Copy HTML** — copy the rendered document as rich HTML with a plain-text
  fallback.
- **Copy text** — copy the rendered document as plain text.
- **Print** — generate the PDF layout and open the system print view.

Every export starts from a native save dialog with a suggested file name based
on the document title. If the title is empty or contains path separators, the
name is sanitized. Cancelling the dialog writes nothing and leaves the document
untouched.

If the destination has no extension, the format's extension is appended:

| Format | Extension |
|---|---|
| Markdown | `.md` |
| HTML | `.html` |
| PDF | `.pdf` |

An extension supplied by the user is preserved, so exporting `notes.txt` as
HTML writes `notes.txt` rather than silently renaming it.

## Markdown export

Markdown export is a passthrough: the editor buffer is written unchanged. This
preserves the source, including front matter and any syntax the preview does not
render.

The export is written through `workspace::atomic::atomic_write_text`, so a
failed write leaves the previous destination intact and never produces a partial
file.

## Standalone HTML export

HTML export uses the same `PreviewDocument` as the preview and produces one
self-contained file:

- The stylesheet is generated from the active theme tokens and the preview
  typography, and embedded in a `<style>` element.
- Local images referenced relative to the document folder are read and embedded
  as base64 data URIs. Missing images become a visible placeholder with the alt
  text.
- Remote images remain remote URLs; links keep their destination for the reader.
- Block math is embedded as inline SVG rendered by `iced_math`, using the active
  theme's text color.
- Mermaid blocks are rendered to SVG by `merman` and embedded inline.
- Fenced code is highlighted with `syntect`. Light themes use
  `base16-ocean.light` and dark themes use `base16-ocean.dark`.
- Headings, lists, task lists, definition lists, tables, quotes, footnotes,
  rules, and safe inline/block HTML are rendered as semantic elements.
- Scripts, event-handler attributes, and `javascript:`/`vbscript:`/`data:` link
  targets are neutralized before output.

The result has no stylesheet, script, or font dependency on the network. A link
to an external page still points at that page, but opening the document itself
does not require a network connection.

Theme changes take effect on the next export because the request captures the
current `ThemeTokens`, not a cached stylesheet.

## PDF export

PDF export maps the preview model onto `pdfrs`:

- Page size comes from `settings.page_size` (`letter` or `a4`).
- Margins come from `settings.page_margin`, clamped to 18–144 points.
- Font size comes from the preview typography and is clamped to 7–24 points.
- Headings, paragraphs, inline emphasis, lists, task lists, tables, quotes,
  code blocks, rules, and footnotes become corresponding `pdfrs` elements.
- Code blocks use `pdfrs`'s syntax-highlighted monospace layout.
- Display math uses `pdfrs`'s display-math layout from the same source that the
  preview renders to SVG.
- Mermaid diagrams are rendered to SVG by `merman`, rasterized through `resvg`
  at up to 1200 px wide, flattened onto white as an RGB PNG, and embedded. The
  flattening step is required because `pdfrs` embeds three-component PNGs, not
  RGBA images.
- Tables and code blocks are paginated by `pdfrs`; long documents continue onto
  new pages instead of being clipped.

PDF output is content-equivalent to the preview, not pixel-identical. Line
breaks, page breaks, and exact glyph metrics can differ.

## Copy as HTML and plain text

Rich copy uses the same HTML renderer but writes only the document fragment,
without the surrounding `<!DOCTYPE html>`, `<html>`, `<head>`, or `<style>`
elements. This is what rich-text editors expect when pasting.

The clipboard payload contains both forms:

1. An HTML fragment with headings, lists, links, emphasis, code, and other
   semantic markup.
2. A plain-text rendering of the document as the fallback.

If the platform rejects HTML clipboard data, `arboard` falls back to plain text
rather than failing the copy action. Copying plain text directly is also
available from the toolbar.

## Print

Printing generates the same PDF layout as PDF export in a temporary file and
hands it to the operating system:

- macOS opens the PDF in Preview, which exposes the system print panel for the
  document.
- Windows uses the `Print` verb for the PDF.
- Linux opens the PDF with `xdg-open`.

The document is not modified by printing. Cancelling in the system viewer or
print panel creates no print job and leaves the document open. Because the
final dialog belongs to the operating system, print cancellation is verified
manually rather than in the automated test suite.

## Failure handling

All export failures are reported through `ExportError` and never close the
document or the application. A failed export:

1. Sets an error message in the status bar.
2. Opens a modal with the failure reason.
3. Offers **Choose another location**, which starts the save dialog again.
4. Leaves the current document, cursor, and unsaved edits intact.

Unwritable folders, missing parent directories, invalid paths, PDF generation
errors, and clipboard/print errors all follow this path. Writes are atomic, so a
failure cannot truncate an existing destination.

## Consistency

The preview, HTML, PDF, print, and copy paths all consume the same
`PreviewDocument`. `ExportRequest` captures the source, preview model, base
directory, title, theme, typography, and PDF settings at export time, so an
export reflects the current editor buffer even when live preview is paused for a
large file.

Tests compare the exported surfaces:

- `tests/export_spec.rs` snapshots the HTML fragment and checks standalone
  output, theme colors, local images, missing images, Markdown passthrough,
  cancellation, failures, page size/margins, math, diagrams, and shared text.
- `export::tests::preview_html_and_pdf_share_the_same_content` checks that the
  preview model produces matching headings, lists, links, code, math, and
  diagram elements for HTML and PDF.
- `export::pdf::tests` cover page sizes, margins, long documents, RGB diagram
  embedding, and SVG-to-PNG conversion.
- `export::html::tests` cover semantic blocks, theme CSS, unsafe links, local
  images, math, Mermaid, footnotes, and standalone output.

Run the export checks with:

```bash
cargo test -p markmydown --test export_spec
cargo test -p markmydown --lib export::
```

## Limitations

- PDF is content-equivalent to the preview, not pixel-identical.
- Remote images are not downloaded during export; they stay as URLs.
- Inline math remains literal text in v1 (only `$$...$$` display math is
  typeset).
- The print dialog is provided by the operating system's PDF viewer; the
  application cannot cancel a dialog it does not own.
- HTML export does not rewrite links to local Markdown documents; they keep
  their source URLs.
