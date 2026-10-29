# Theme and UI conventions

`markmydown` uses one token model for every surface. The same colors and
typography values drive the Iced widgets, the editor canvas, the preview, the
HTML export, and the PDF layout. This document describes the tokens, theme
registry, settings panel, and the UI conventions that keep the application
consistent and keyboard-first. It covers
[`specs/app-appearance/spec.md`](../openspec/changes/add-markmydown-v1/specs/app-appearance/spec.md).

## Token model

`theme::tokens::ThemeTokens` contains:

| Token | Used by |
|---|---|
| `background` | Window, editor canvas, preview page, exported page |
| `surface` | Sidebar, toolbars, status bar, code block background |
| `surface_raised` | Dialogs, settings card, notices |
| `text` | Body text in the editor, preview, and export |
| `muted` | Secondary labels, line numbers, placeholders |
| `accent` | Active controls, selection outlines, links in controls |
| `border` | Panel dividers, table borders, code fences |
| `selection` | Text selection and search highlights |
| `code_background` | Fenced and inline code |
| `code_text` | Code text before syntax colors |
| `link` | Preview and export links |
| `heading` | Preview and export headings |
| `quote` | Blockquote text |
| `success`, `warning`, `error` | Status, notifications, validation |
| `is_dark` | Selects the light or dark syntax-highlighting palette |

`ThemeTokens::to_iced_theme` maps the core colors to an Iced palette.
`theme::css::stylesheet` emits the same values as CSS custom properties for
offline HTML export. Adding a token requires updating all four surfaces; the
`stylesheet_exports_every_theme_token` test fails if the CSS generator drops
one.

## Theme registry

`theme::registry` contains eight themes:

- `light` and `dark` are the quick choices.
- `system` resolves to light or dark from the operating-system preference.
- The gallery adds Sepia, Nord, Solarized Light, Solarized Dark, Catppuccin
  Latte, and Catppuccin Mocha.

The registry is immutable and lazily initialized. `resolve(id, system_dark)`
returns a theme for any id and falls back to light/dark when a saved id is
unknown. Settings validation uses the same registry, so a removed theme cannot
leave the app without a palette.

System mode has two inputs:

1. `tokens::system_is_dark()` reads the current preference synchronously at
   boot.
2. `iced::system::theme_changes()` sends `Message::SystemThemeChanged` while
   the app is running.

A system change only re-resolves the palette when `settings.theme` is
`system`. A user-picked theme is never overridden by the OS.

## Theme preview

The settings panel renders each gallery theme as a card with a color swatch.
The interaction model is:

- **Hover** a card to preview it. The live tokens change, but
  `settings.theme` is untouched.
- **Preview** explicitly previews a card for keyboard/pointer users.
- **Apply** persists the theme and clears the preview state.
- **Leave** the card (or close the panel) to restore the previously applied
  palette. The original tokens are kept in `preview_theme_backup`.

Previews do not write the settings file and do not change the editor buffer or
export request. The exported output always uses the applied theme, which is the
theme captured by `ExportRequest` at export time.

## Typography

`theme::tokens::Typography` is stored twice in `Settings`:

- `editor_typography`
- `preview_typography`

Each value has a font family, font size, line height, and maximum text width.
The editor adapter applies its own value to the code editor; the preview and
HTML export use the preview value. Changing one never changes the other.

Defaults:

| Surface | Family | Size | Line height | Max width |
|---|---|---:|---:|---:|
| Editor | SF Mono | 14 px | 1.55 | unlimited |
| Preview | Georgia | 17 px | 1.65 | 760 px |

Settings validation clamps font sizes to 8–32 px (editor) and 9–40 px
(preview), line heights to 1.0–2.5, and widths to 0–2400 px. Empty font
families fall back to the default family. The settings panel has a reset action
for each surface and a reset-all action with a confirmation dialog.

## Settings persistence

Settings live in `AppPaths::settings_file()` (the platform config directory)
with `schema_version = 1`:

```json
{
  "schema_version": 1,
  "theme": "system",
  "layout": "side-by-side",
  "editor_typography": { "font_family": "SF Mono", "font_size": 14.0 },
  "preview_typography": { "font_family": "Georgia", "font_size": 17.0 },
  "page_size": "letter",
  "page_margin": 72.0
}
```

Every new field has a serde default so older files continue to load. Writes go
through the atomic writer.

If the file cannot be parsed:

1. The file is renamed to `settings.corrupt-<pid>.json` for diagnosis.
2. Defaults are used for the session.
3. The warning is logged and shown in the status bar.

If the schema is newer than the application supports, the same preservation
and defaults path is used. A missing file is not an error; defaults are used
silently. Reset-all opens a confirmation dialog, then restores defaults,
persists them, and reapplies the theme, layout, panes, and focus mode.

## Settings panel

Open the panel with **Settings** in the toolbar or `Cmd/Ctrl+,`. The panel is a
modal card with:

- Theme quick choices: System, Light, Dark.
- A gallery of theme cards with swatches, Preview, and Apply.
- Editor typography: family, size, line height, reset.
- Preview typography: family, size, line height, text width, reset.
- Reset all settings.
- Close (`Esc` also closes and restores an unconfirmed preview).

The panel is scrollable and capped to the window size, so it remains usable on
the minimum 760 × 520 window.

## Welcome document and cheat sheet

Both built-in documents are static Markdown parsed by the normal preview
pipeline. They never require a network connection.

- The welcome document demonstrates headings, emphasis, links, lists, task
  lists, code, tables, display math, and Mermaid diagrams.
- The cheat sheet lists Markdown syntax and the fixed keyboard shortcuts.

On first launch without a workspace, the welcome document opens over the empty
workspace and `onboarding_seen` is saved. It can be reopened with **Read the
welcome guide** or **Welcome**. The cheat sheet opens from the toolbar,
`Cmd/Ctrl+/`, or the welcome screen. Closing either overlay reveals the
previous document and preserves its cursor and scroll.

## Quick open

`ui::palette::QuickOpen` is a fuzzy subsequence matcher with prefix and
consecutive-match bonuses. It is populated from `WorkspaceTree::files()` and
shows the title and workspace-relative path.

- Open with `Cmd/Ctrl+P` or **Quick open**.
- Type to filter.
- `Up`/`Down` move the selection; `Enter` or a click opens the document.
- `Esc` or **Close** dismisses the palette.

The palette focuses its search field when opened and never changes files that
are not in the workspace.

## Notifications and errors

`ui::notifications::NotificationCenter` keeps at most four notices. Success,
info, and error notices are shown in the top-right corner and expire after six
seconds; each can be dismissed individually.

Theme changes, typography resets, settings reset, and successful exports push a
success notice. Export failures push an error notice and also open an export
failure dialog that offers another destination. Destructive settings actions
use the same confirmation dialog as file deletion and tab closing, so an error
never leaves the app without an obvious next step.

## Keyboard conventions

| Shortcut | Action |
|---|---|
| `Cmd/Ctrl+S` | Save |
| `Cmd/Ctrl+F` | Find |
| `Cmd/Ctrl+H` | Replace |
| `Cmd/Ctrl+P` | Quick open |
| `Cmd/Ctrl+,` | Settings |
| `Cmd/Ctrl+/` | Cheat sheet |
| `Esc` | Close the top overlay, dialog, or search panel |

The editor owns its text-editing shortcuts; the app-level listener only handles
keys the editor leaves unhandled. Quick open adds arrow and Enter handling only
while the palette is visible, so those keys behave normally in the editor.

## Tests

```bash
cargo test -p markmydown --test appearance_spec
cargo test -p markmydown --lib theme::
cargo test -p markmydown --lib app::tests
cargo test -p markmydown --lib config::settings
```

Coverage includes theme resolution and fallback, system-mode following,
preview/apply/cancel behavior, typography independence and clamping, settings
round-trip and corruption recovery, reset confirmation, welcome/cheat-sheet
content, quick-open filtering, notifications, and snapshots of the built-in
cheat sheet and theme token values.
