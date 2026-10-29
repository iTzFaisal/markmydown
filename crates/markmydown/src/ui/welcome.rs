//! Built-in welcome document and Markdown cheat sheet.
//!
//! Both are static Markdown strings parsed by the normal preview pipeline, so
//! they render offline and exercise the same code as user documents.

pub const WELCOME: &str = r#"# Welcome to markmydown

A fast, native, offline Markdown workbench. Your files stay on this computer.

## Start here

1. Open a folder from the toolbar or sidebar.
2. Pick a Markdown file, or create a new one.
3. Edit on the left and read on the right.

## Markdown works as you expect

Write **bold**, *italic*, ~~strikethrough~~, `inline code`, and [links](https://commonmark.org).

- Lists keep their place
- Task lists track work
  - [x] Open a folder
  - [ ] Write something

> Quotes are quiet.
> They can span lines.

```rust
fn main() {
    println!("Hello from a highlighted code block");
}
```

## Tables and math

| Feature | Status |
|:--------|-------:|
| Preview | ready |
| Export  | ready |

$$
E = mc^2
$$

```mermaid
flowchart LR
    Source[Markdown] --> Preview
    Preview --> Export[HTML / PDF]
```

## Keep exploring

Use Quick Open (Cmd/Ctrl+P) to jump between documents, and open the cheat
sheet for the full shortcut list.
"#;

pub const CHEAT_SHEET: &str = r#"# Markdown cheat sheet

## Text

| Syntax | Result |
|---|---|
| `**bold**` | **bold** |
| `*italic*` | *italic* |
| `~~strike~~` | ~~strike~~ |
| `` `code` `` | `code` |
| `[text](https://example.com)` | link |
| `![alt](image.png)` | image |

## Blocks

````markdown
# Heading 1
## Heading 2

- bullet
1. numbered
- [ ] task
- [x] done

> quote

```rust
fn main() {}
```

$$
E = mc^2
$$
````

## Editor shortcuts

| Shortcut | Action |
|---|---|
| Cmd/Ctrl+S | Save |
| Cmd/Ctrl+P | Quick open |
| Cmd/Ctrl+F | Find |
| Cmd/Ctrl+H | Replace |
| Cmd/Ctrl+, | Settings |
| Cmd/Ctrl+/ | Cheat sheet |
| Cmd/Ctrl+B | Bold |
| Cmd/Ctrl+I | Italic |
| Cmd/Ctrl+K | Link |
| Cmd/Ctrl+1…6 | Heading level |
| Cmd/Ctrl+Shift+8 | Bullet list |
| Cmd/Ctrl+Shift+7 | Numbered list |
| Cmd/Ctrl+Shift+9 | Task list |
| Cmd/Ctrl+Shift+. | Quote |
| Cmd/Ctrl+` | Inline code |
| Cmd/Ctrl+Shift+` | Code block |
| Tab / Shift+Tab | Indent / outdent |

## Layout and focus

Use the toolbar to switch between Editor, Split, Preview, and Read. Focus mode
dims everything except the current line and keeps the caret centered.
"#;
