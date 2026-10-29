# Accessibility and input audit

This document records the v1 keyboard, IME, and RTL audit required by
[`specs/markdown-editor/spec.md`](../openspec/changes/add-markmydown-v1/specs/markdown-editor/spec.md)
and the release-readiness tasks. It documents gaps rather than claiming full
accessibility.

## Keyboard operation

Core document work is possible without a mouse:

| Action | Shortcut |
|---|---|
| Save | `Cmd/Ctrl+S` |
| Find / replace | `Cmd/Ctrl+F`, `Cmd/Ctrl+H` |
| Quick open | `Cmd/Ctrl+P` |
| Settings | `Cmd/Ctrl+,` |
| Cheat sheet | `Cmd/Ctrl+/` |
| Bold / italic / strikethrough | `Cmd/Ctrl+B`, `Cmd/Ctrl+I`, `Cmd/Ctrl+Shift+X` |
| Headings 1–6 | `Cmd/Ctrl+1` … `Cmd/Ctrl+6` |
| Bullet, numbered, task list | `Cmd/Ctrl+Shift+8/7/9` |
| Quote, inline code, code block | `Cmd/Ctrl+Shift+.`, `Cmd/Ctrl+\``, `Cmd/Ctrl+Shift+\`` |
| Link / image | `Cmd/Ctrl+K`, `Cmd/Ctrl+Shift+I` |
| Table | `Cmd/Ctrl+T` |
| Indent / outdent | `Tab` / `Shift+Tab` |
| Close top overlay | `Esc` |

Quick open supports `Up`/`Down` to move and `Enter` to open. The editor owns
text-editing keys, undo/redo, selection, and clipboard shortcuts. The app-level
listener only sees keys the editor leaves unhandled, which prevents duplicate
handling of `Cmd/Ctrl+B` and similar editor commands.

### Verified without a mouse

- Open a folder via the native file dialog (keyboard-navigable).
- Open documents through quick open.
- Edit, format, find, replace, save, and undo/redo.
- Switch between editor, split, preview, and read layouts via toolbar buttons
  (mouse) or the corresponding menu-less shortcuts where provided.
- Close overlays and palettes with `Esc`.

### Keyboard gaps

| Gap | Impact | Workaround |
|---|---|---|
| No next/previous tab shortcut | Tab switching needs a click | Use quick open to reopen a document; the active tab can be changed with the mouse. |
| Sidebar tree is pointer-driven | Keyboard users cannot expand folders or select files from the tree | Quick open searches all supported files directly. |
| Outline navigation is pointer-driven | Keyboard users cannot jump by heading from the outline | Editor find can locate heading text; outline buttons need a pointer. |
| Settings sliders are pointer-oriented | Theme, size, and line-height adjustments may require a mouse | Theme shortcuts are available through the settings panel only; defaults are usable. |
| No focus ring audit / AccessKit | Screen readers cannot inspect widgets | Native screen-reader support is deferred until Iced provides AccessKit integration. |

Iced 0.14 does not expose AccessKit, so the application cannot provide a
reliable screen-reader tree. This is a framework limitation, not a choice to
hide content from assistive technology. Keyboard-first operation and the
cheat sheet are the current mitigations.

## IME

The vendored editor implements composition events for IME input. The headless
spike sends committed Arabic and CJK text through the adapter and asserts that
each composition appears exactly once, with no duplicated commit. The editor
only accepts composition while the canvas has focus.

Platform notes:

- Candidate-window placement is provided by `winit` and the operating system;
  it is not rendered by the app.
- Composition start/update/end behavior can vary between macOS, Windows, and
  Linux IMEs. The headless test covers duplicate-commit protection, not every
  OS candidate window.
- If a platform IME sends both a composition update and a committed character
  in an unexpected order, the editor's commit guard is the defense; manual
  testing is still required on each OS.

## RTL and bidirectional text

- Text is stored and edited in logical order. Copy, save, export, and search use
  logical offsets.
- The canvas does not perform Unicode bidi reordering, so mixed RTL/LTR text is
  not displayed with visual bidi runs. Arabic/Hebrew text remains correct in
  the file; only its on-screen ordering is limited.
- Caret movement and selection are logical character offsets rather than
  visual bidi levels.
- The preview and HTML export use the platform text shaper and preserve the
  source order; exported files remain valid.

## What to verify on each platform

1. Type Arabic and CJK text with an IME; confirm each character appears once.
2. Move the caret through mixed RTL/LTR text; confirm selection offsets match
   the logical characters.
3. Use only the keyboard for open-folder, quick-open, edit, find, save, and
   close-overlay flows.
4. Run a screen reader against the app and record the missing AccessKit tree as
   a known gap.

## Summary

Keyboard editing and document workflows are usable without a mouse. Tab
switching, the sidebar tree, outline navigation, and settings sliders still
have pointer-oriented gaps. IME duplicate-commit protection is tested; RTL
visual reordering and screen-reader support are not available in v1. These gaps
are documented in the README and tracked for a future release.
