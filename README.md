# markmydown

A fast, native, offline-first Markdown workbench built in Rust with Iced. Open a
local folder, edit plain Markdown files, preview GFM with math and Mermaid, and
export portable Markdown, HTML, or PDF. No webview, no accounts, no cloud
storage.

## Features

- Local-folder workspace with a file tree, tabs, autosave, atomic writes,
  external-change detection, conflict handling, crash recovery, and session
  restore.
- Native editor with Markdown syntax highlighting, formatting actions, smart
  lists, find/replace, outline, status bar, focus mode, and editor/split/
  preview/read layouts.
- Native preview with GFM, tables, task lists, footnotes, definition lists,
  sub/sup, mark, emoji, code highlighting, block math, Mermaid diagrams,
  relative images, sanitized HTML, scroll sync, and read mode.
- Theme gallery with light, dark, system-following, Sepia, Nord, Solarized, and
  Catppuccin themes, plus independent editor and preview typography.
- Welcome document, searchable cheat sheet, fuzzy quick open, notifications,
  and local diagnostics.
- Export to Markdown, standalone HTML, and paginated PDF; copy as rich HTML or
  plain text; print through the system PDF viewer.
- No network required for editing or export. Update checks are explicit and
  request release metadata only.

## Requirements

- Rust stable with edition 2024 support (Rust 1.85 or newer).
- macOS 12+, Windows 10/11, or a Linux desktop with Wayland or X11.
- Linux build packages: `libasound2-dev`, `libfontconfig1-dev`,
  `libgl1-mesa-dev`, `libwayland-dev`, `libx11-dev`, `libxcb1-dev`,
  `libxcursor-dev`, `libxi-dev`, `libxinerama-dev`, `libxkbcommon-dev`,
  `libxrandr-dev`, and `pkg-config`.

## Build and run

```bash
git clone https://github.com/iTzFaisal/markmydown.git
cd markmydown
cargo build -p markmydown --release
cargo run -p markmydown --release
```

Check the built version with:

```bash
markmydown --version
```

Open a folder with the toolbar or sidebar, or pass a folder or document on the
command line:

```bash
markmydown ~/notes
markmydown ~/notes/today.md ~/notes/ideas.markdown
```

Multiple file paths open as tabs. File-association launches from Finder,
Explorer, or the Linux file manager use the same path handling.

### Tests

```bash
cargo test -p markmydown
cargo test -p markmydown --test end_to_end_spec
cargo test -p markmydown --release --test performance_spec -- --ignored --nocapture
```

CI runs formatting, clippy, tests, and a build on macOS, Windows, and Ubuntu.
The performance benchmark is ignored by default because it is timing-sensitive;
see [`docs/performance.md`](docs/performance.md).

## Usage

- **Open a folder** to create a workspace. Supported files are `.md`,
  `.markdown`, `.mdown`, and `.txt`.
- **Edit** on the left; the preview updates after a short debounce. Use the
  toolbar or shortcuts to switch layouts.
- **Save** with `Cmd/Ctrl+S`; autosave runs after an idle interval. External
  edits reload clean tabs and show a conflict bar for dirty ones.
- **Quick open** with `Cmd/Ctrl+P`; type to fuzzy-search and press Enter.
- **Settings** with `Cmd/Ctrl+,` for themes and editor/preview typography.
- **Cheat sheet** with `Cmd/Ctrl+/` for Markdown syntax and shortcuts.
- **Export** with the toolbar: Markdown, standalone HTML, or PDF. HTML embeds
  the active theme, local images, math, and diagrams. PDF uses the configured
  page size and margins.
- **Copy** as rich HTML or plain text. **Print** hands the PDF layout to the
  system PDF viewer.
- **Recover** unsaved work from the prompt shown after an unexpected exit.

## Project layout

```text
crates/markmydown/src/
  app.rs          App state, messages, update, view
  workspace/      tree, documents, tabs, watcher, recovery, session
  editor/         adapter over the vendored editor and Markdown behaviors
  preview/        comrak parser, model, renderer, math, Mermaid, cache
  export/         Markdown, HTML, PDF, clipboard, print
  theme/          tokens, gallery registry, CSS generation
  ui/             toolbar, sidebar, settings, palette, dialogs, status
  config/         paths, settings, logging, launch paths, updates
```

## Packaging

`cargo-packager` produces the platform installers and file associations:

```bash
cargo install cargo-packager --locked
cd crates/markmydown
cargo packager --release
```

Artifacts are written to `target/packager`. The release workflow
(`.github/workflows/release.yml`) builds macOS, Windows, and Linux packages,
signs them when the platform secrets are configured, verifies signatures, and
publishes them to GitHub Releases. macOS uses Developer ID signing plus
notarization; Windows uses `signtool` with a certificate thumbprint.

Unsigned local builds are still fully functional. Signed release verification
requires the platform signing credentials described in the workflow file.

### Release candidates without signing certificates

Push a pre-release tag such as `v0.1.0-rc.1`, or run the **Release** workflow
manually with that tag. The workflow builds unsigned macOS, Windows, and Linux
artifacts, uploads them, and marks tags containing `-rc` as pre-releases.

Without Apple Developer and Windows signing certificates:

- macOS artifacts are unsigned and not notarized; users must use the
  Gatekeeper "Open Anyway" flow.
- Windows artifacts are unsigned; SmartScreen may warn.
- Linux artifacts run without signing.
- Version, file associations, and update-feed checks can still be verified.
- Signature verification remains pending and must be repeated once
  certificates are configured.

Run the unsigned packages:

```bash
# macOS (after copying markmydown.app to /Applications)
xattr -dr com.apple.quarantine /Applications/markmydown.app
open /Applications/markmydown.app

# Linux AppImage
chmod +x markmydown-*.AppImage
./markmydown-*.AppImage
```

On macOS you can also right-click the app and choose **Open** the first time.
On Windows, choose **More info → Run anyway** if SmartScreen warns. Locally
built binaries run without these steps because they carry no quarantine flag.

Add these repository secrets to produce signed artifacts:

| Secret | Purpose |
|---|---|
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` | Developer ID `.p12` certificate and password |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: …` identity |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Notarization credentials |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` | Base64 `.pfx` and password |

### Versioning and releases

The app version lives in the workspace manifest:

```toml
# Cargo.toml
[workspace.package]
version = "0.1.1"
```

The app crate inherits it with `version.workspace = true`. To bump:

```bash
# 1. Edit the workspace version.
# 2. Refresh the lockfile entry.
cargo check -p markmydown
# 3. Commit the manifest and lockfile.
git add Cargo.toml Cargo.lock
git commit -m "release: v0.1.1"
# 4. Push the tag that triggers the Release workflow.
git tag v0.1.1
git push origin main --tags
```

For an unsigned release candidate, use a pre-release version and tag, such as
`0.1.1-rc.1` and `v0.1.1-rc.1`. The workflow marks tags containing `-rc` as
pre-releases and builds the same installers without requiring signing secrets.
You can also run it from the command line:

```bash
gh workflow run Release --ref main -f tag=v0.1.1-rc.1
```

## Documentation

- [`docs/workspace.md`](docs/workspace.md) — file safety, tabs, sessions
- [`docs/editor.md`](docs/editor.md) — editor adapter, shortcuts, vendored patch
- [`docs/preview.md`](docs/preview.md) — preview model and extensions
- [`docs/export.md`](docs/export.md) — export, copy, print, failures
- [`docs/theme.md`](docs/theme.md) — theme tokens, settings, UI conventions
- [`docs/performance.md`](docs/performance.md) — benchmark results
- [`docs/accessibility.md`](docs/accessibility.md) — keyboard, IME, RTL, gaps
- [`docs/security.md`](docs/security.md) — security and privacy audit

## License

MIT.
