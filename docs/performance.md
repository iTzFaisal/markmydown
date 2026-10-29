# Performance

The v1 performance budget is measured on a 1 MB Markdown document with a warm
release build. Run the benchmark with:

```bash
cargo test -p markmydown --release --test performance_spec -- --ignored --nocapture
```

## Results

Environment: macOS, release profile, 2026-10-29.

| Operation | Result | Budget |
|---|---:|---:|
| Open a 1 MB document | 8.4 ms | < 200 ms |
| Preview update (parse + model) | 27.6 ms | < 80 ms |
| Keystroke (insert + backspace, incremental editor) | 0.7 µs | < 16 ms |
| HTML export | 261 ms | recorded |
| PDF export | 469 ms | recorded |

The preview and editor measurements meet the release thresholds. HTML and PDF
export are not latency-critical user interactions; their times are recorded so a
regression is visible across releases.

## What is measured

- **Open** covers reading and decoding the file, front matter parsing, hashing
  for change detection, and creating the document tab.
- **Preview update** covers the comrak parse and the `PreviewDocument` model
  build, not Mermaid or math rendering (those are cached separately).
- **Keystroke** uses the vendored editor's incremental edit harness on the same
  1 MB buffer; it measures one character insert/remove, not full-frame paint.
- **Export** covers HTML rendering and PDF element generation/writing,
  including one representative code block pattern per section.

## Optimizations in place

- Preview parsing is debounced (default 80 ms).
- Documents above 1 MB pause live preview until the user refreshes it.
- Math and Mermaid results are cached by content hash.
- The preview virtualizes long documents by block.
- Editor highlighting is incremental and only tokenizes the visible work.
- Exports build from a single preview model rather than re-parsing Markdown.
- Writes are atomic but do not re-read the file after a save.

## Reproducing

The benchmark is ignored in normal test runs because it is intentionally large
and timing sensitive. Run it in release mode and on an otherwise idle machine.
The test asserts the open, preview, and keystroke budgets and prints export
times.
