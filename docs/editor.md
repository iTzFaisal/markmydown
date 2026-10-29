# Editor adapter and vendored patch surface

This document describes the source editor milestone: how markmydown wraps the
vendored `iced-code-editor`, which Markdown behaviors it adds, how focus,
search, outline, and layout work, and what must be preserved when the vendored
crate is updated. It maps to
[`specs/markdown-editor/spec.md`](../openspec/changes/add-markmydown-v1/specs/markdown-editor/spec.md).

## Module map

| Module | Responsibility |
|---|---|
| `editor::mod` (`EditorPane`) | One editor widget per open tab; theme, typography, formatting, smart editing, search, and focus forwarding. |
| `editor::markdown` | Pure Markdown transformations for formatting, list continuation, indentation, and pasted URLs. |
| `editor::search` | Case-sensitive, regex, and replace-all matching; returns one `TextEdit` per operation so the editor records one undo step. |
| `editor::outline` | ATX and setext heading extraction, ignoring fenced code and front matter. |
| `editor::stats` | Word, character, reading-time, cursor, and selection counts, including CJK word counting. |
| `ui::search` | Find/replace panel state and controls. |
| `ui::outline` | Clickable heading list. |
| `ui::status` | Word/character/reading-time, selection, progress, and cursor display. |
| `app` | Fixed shortcuts, layout modes, resizable panes, pane persistence, and wiring to documents. |

## Vendored editor and patch surface

The workspace vendors `iced-code-editor` at `vendor/iced-code-editor` under its
MIT license. markmydown depends on it by path, so the app and the editor build
together and one Iced version is used.

The intentional patch surface is deliberately small:

1. **Host editing API** (`src/canvas_editor/host_api.rs`):
   - normalized selection access,
   - selection-preserving edits as single undo steps,
   - whole-document replacement for replace-all and safe reloads,
   - programmatic selection, caret, line, and scroll access.
2. **Focus mode** (`src/canvas_editor/host_api.rs`,
   `editing/cursor.rs`, `render/text.rs`):
   - `set_focus_mode` and `set_focus_dim_strength`,
   - per-line opacity for non-current text,
   - caret-centering scroll policy while focus mode is active.
3. **Host search decorations** (`src/canvas_editor/host_api.rs`,
   `render/overlays.rs`, `render/canvas.rs`):
   - `set_host_highlights` accepts ranges with their own end positions, so
     regex matches of different lengths can be highlighted,
   - `clear_host_highlights` removes them.
4. **Markdown syntax resolution** (`src/canvas_editor/render/highlighting.rs`):
   - the `markdown` alias resolves to syntect's `Markdown` syntax,
   - the palette follows the editor background (light vs dark).
5. **Command-modified text suppression**
   (`src/canvas_editor/input/events.rs`):
   - text events with Command/Ctrl held are not inserted, so application
     shortcuts such as Cmd+B and Cmd+K are handled by the host instead of
     leaking a character into the document.

Everything else in the crate is upstream code. `EditorPane` is the only place
the rest of the application touches the vendored types.

### Updating the vendored crate

1. Replace `vendor/iced-code-editor` with the new upstream tree.
2. Re-apply the five patch areas listed above. Compare against the previous
   vendor tree rather than reapplying a generated patch blindly; upstream may
   have moved the same code.
3. Keep `CodeEditor::new(text, "markdown")`, the host editing methods, focus
   methods, host-highlight methods, and the Command-modified text guard in the
   public or `pub(crate)` API.
4. Run `cargo test -p iced-code-editor --lib` and the adapter tests:
   `cargo test -p markmydown --test editor_spec`.
5. Run `cargo test --workspace` and `cargo clippy --workspace --all-targets`.
6. Check the focus tests: non-current lines must dim, the caret line must stay
   fully opaque, and disabling focus mode must restore normal opacity and
   scrolling.
7. Check the markdown highlighting test: the resolved syntax must be
   `Markdown` and a palette must be selected.

The patch surface is small on purpose. If an update requires larger changes,
prefer adapting `EditorPane` first and patching the vendored crate only when
the behavior cannot be expressed through its public API.

## Formatting and shortcuts

| Action | Shortcut |
|---|---|
| Bold | Cmd/Ctrl+B |
| Italic | Cmd/Ctrl+I |
| Heading 1–6 | Cmd/Ctrl+1 … Cmd/Ctrl+6 |
| Strikethrough | Cmd/Ctrl+Shift+X |
| Bullet list | Cmd/Ctrl+Shift+8 |
| Ordered list | Cmd/Ctrl+Shift+7 |
| Check list | Cmd/Ctrl+Shift+9 |
| Blockquote | Cmd/Ctrl+Shift+. |
| Inline code | Cmd/Ctrl+` |
| Code block | Cmd/Ctrl+Shift+` |
| Table | Cmd/Ctrl+T |
| Link | Cmd/Ctrl+K |
| Image | Cmd/Ctrl+Shift+I |
| Find | Cmd/Ctrl+F |
| Replace | Cmd/Ctrl+H |
| Save | Cmd/Ctrl+S |
| Close the search panel | Esc |

Formatting wraps the selection, or inserts a placeholder when there is no
selection. Link and image open a small URL dialog; an empty URL is rejected.
Heading and list actions toggle off when every selected line already has that
style. Lists number sequentially. Table inserts a two-column skeleton and puts
the caret in the first cell.

The editor consumes the keys it owns (save, undo/redo, clipboard, built-in
navigation). markmydown listens only for ignored key events, so fixed
application shortcuts do not duplicate editor actions. The editor's built-in
find messages are intercepted and routed to the application search panel so
regex and replace-all share one implementation.

## Smart editing

- **Enter** at the end of a list item inserts a new item with the same marker;
  ordered lists increment. Enter on an empty item removes the marker and leaves
  the list.
- **Tab** indents a list item by four spaces; **Shift+Tab** outdents it.
- **Paste** over a selection turns the selection into a link when the pasted
  text is an `http`, `https`, `mailto`, or `www` URL. Pasting without a
  selection or with ordinary text keeps the editor's normal paste.

Each behavior is a pure transformation in `editor::markdown` and is applied
through `EditorPane::apply_edit`, so it is one undo step.

## Find and replace

The search panel supports:

- case-insensitive search by default and a case-sensitive toggle;
- literal search by default and a regex toggle;
- next/previous navigation with wraparound;
- replace current and replace all;
- capture-group replacement (`$1`, `$2`) when regex is enabled;
- an inline error for an invalid regex, with no document change;
- host decorations for all matches, with the current match in the accent color.

Replace-all builds one `TextEdit`, then calls the editor's whole-document
replacement, so a single undo restores the previous document. Replace-current
does the same for one match.

## Outline and status bar

The outline is generated from the current document text in `editor::outline`:
ATX headings, setext headings, fenced code exclusion, and front matter
exclusion. Selecting a heading moves the caret to that source line. The
outline panel is toggled from the toolbar and its visibility is persisted.

The status bar reports:

- word count, character count, and reading time at 200 words per minute;
- selection word and character counts while a selection exists;
- one-based cursor line and column.

CJK ideographs and kana count as words; whitespace-only selections do not
produce selection statistics.

## Focus mode

Focus mode uses the vendored opacity hook. Lines at the caret and one line
either side stay fully opaque; farther lines fade progressively. Movement
keeps the caret vertically centered by using the focus scroll policy instead
of the ordinary "scroll only when out of view" policy. Disabling focus mode
restores the normal appearance and scrolling without changing the document.
The focus state is persisted in settings.

## Layouts and resizable panes

- **Editor**: the editor fills the content area; preview is hidden.
- **Split**: editor and preview are separate panes in an Iced `pane_grid`.
- **Preview**: the preview fills the content area; the editor is hidden.
- **Read**: the reading layout (preview-only in this milestone).

The split ratio is stored in `Settings::split_ratio`, restored on launch, and
saved when the splitter is dragged. The selected layout is persisted in
`Settings::layout`. Pane ratios are clamped to 0.2–0.8 so neither pane can be
collapsed accidentally.

## IME and RTL spike

The input-method test sends committed Arabic and CJK text through the adapter
and asserts that each composition appears exactly once. The vendored editor
already guards against duplicate commits and only accepts composition while the
canvas has focus.

Known limitations in this milestone:

- Bidi text is stored and edited in logical order; the canvas does not reorder
  RTL runs visually. This is a rendering limitation, not a data-loss risk.
- Caret movement and selection are logical character offsets, not visual bidi
  levels.
- IME candidate-window placement and platform-specific composition behavior
  depend on `winit` and the OS; the headless test covers commit duplication,
  not candidate-window placement.
- Full keyboard navigation exists for the editor, but screen-reader support
  remains an Iced 0.14 gap (no AccessKit).

## Performance check

`tests/editor_spec.rs` builds a 1 MB Markdown fixture, selects text, types, and
undoes, and asserts the whole sequence completes within the responsive-test
budget. The editor's syntax highlighting is incremental and only tokenizes the
visible prefix, so large documents do not get fully re-highlighted on each
keystroke. The app's preview threshold is handled in the preview milestone.

## Test coverage

The scenarios in the editor spec are covered by:

| Spec scenario | Test |
|---|---|
| Typing | `typing_selection_undo_redo_and_paste_flow_through_the_adapter` |
| Undo and redo | `typing_selection_undo_redo_and_paste_flow_through_the_adapter` |
| Input-method composition | `ime_composition_is_committed_once_for_arabic_and_cjk` |
| Bold, italic, heading, strikethrough, lists, quote, code | `formatting_actions_produce_the_expected_markdown` |
| Link | `table_link_and_image_match_the_spec_scenarios` |
| Table | `table_link_and_image_match_the_spec_scenarios` |
| Continue a list | `smart_editing_continues_indents_and_links_pasted_urls` |
| Indent a list item | `smart_editing_continues_indents_and_links_pasted_urls` |
| Paste a URL over selected text | `smart_editing_continues_indents_and_links_pasted_urls` |
| Find text | `search_supports_case_regex_navigation_and_single_step_replace_all` |
| Replace all | `search_supports_case_regex_navigation_and_single_step_replace_all` |
| Invalid regular expression | `search_supports_case_regex_navigation_and_single_step_replace_all` |
| Outline navigation | `outline_and_stats_cover_navigation_and_selection_counts` |
| Document statistics | `outline_and_stats_cover_navigation_and_selection_counts` |
| Selection statistics | `outline_and_stats_cover_navigation_and_selection_counts` (plus `workspace::document` test) |
| Enable/disable focus mode | `focus_mode_toggles_and_restores_the_editor` and vendored `focus_mode_dims_lines_away_from_the_caret_and_restores_them` |
| Layout modes and pane persistence | `layout_modes_round_trip_and_split_ratio_persists` and `editor_layouts_and_panels_build_a_view` |
| Keyboard shortcuts | `app::tests::fixed_formatting_shortcuts_map_to_actions` and `app::tests::file_and_search_shortcuts_are_fixed` |
| Large document responsiveness | `a_one_megabyte_document_stays_within_the_interaction_budget` |

Run the suite with:

```sh
cargo test --workspace
```

## Known limits in this milestone

- The preview pane is still a placeholder until the preview milestone; the
  layout, splitter, and persistence are in place.
- Read mode uses the preview-only layout; reading typography, outline progress,
  and scroll synchronization arrive with the preview milestone.
- The in-app shortcut reference and command palette arrive in the appearance
  and onboarding milestone.
- The search panel is document-scoped; workspace-wide search is not in v1.
