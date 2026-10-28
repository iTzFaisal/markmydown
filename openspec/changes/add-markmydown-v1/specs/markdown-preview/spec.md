# Spec Delta

## Purpose

Renders Markdown natively into a readable preview with GFM, code highlighting, block math, diagrams, images, sanitization, and a dedicated read mode.

## ADDED Requirements

### Requirement: CommonMark and GFM rendering

The preview SHALL render CommonMark and GitHub Flavored Markdown, including headings, paragraphs, emphasis, lists, task lists, tables, blockquotes, code blocks, links, images, strikethrough, and autolinks.

#### Scenario: Table

- **WHEN** a document contains a GFM table
- **THEN** it renders as a table with aligned columns

#### Scenario: Task list

- **WHEN** a document contains task list items
- **THEN** checked and unchecked states are shown

#### Scenario: Autolink

- **WHEN** a document contains a bare URL
- **THEN** it renders as a link

### Requirement: Extended Markdown syntax

The preview SHALL render footnotes, definition lists, abbreviations, subscript, superscript, highlighted text, emoji shortcodes, and typographic punctuation.

#### Scenario: Footnote

- **WHEN** a document contains a footnote reference and definition
- **THEN** the reference links to the note and the note is shown

#### Scenario: Definition list

- **WHEN** a document contains a definition list
- **THEN** terms and definitions are rendered distinctly

#### Scenario: Emoji shortcode

- **WHEN** a document contains a supported emoji shortcode
- **THEN** the corresponding emoji is shown

### Requirement: Code highlighting

Code blocks SHALL be syntax-highlighted when a language is declared and SHALL provide a copy action.

#### Scenario: Highlight code

- **WHEN** a fenced code block declares a supported language
- **THEN** its content is highlighted

#### Scenario: Copy code

- **WHEN** the user activates copy on a code block
- **THEN** the code is copied to the clipboard

#### Scenario: Unknown language

- **WHEN** a code block declares an unknown language
- **THEN** the code is shown without highlighting and without error

### Requirement: Block math

Display math delimited by `$$...$$` SHALL render as a typeset equation. Inline math is outside this release and SHALL be shown as literal text.

#### Scenario: Display equation

- **WHEN** a document contains a `$$...$$` block
- **THEN** it renders as a typeset equation

#### Scenario: Invalid equation

- **WHEN** an equation cannot be parsed
- **THEN** the raw source is shown with an error indicator

#### Scenario: Inline math

- **WHEN** a document contains `$...$`
- **THEN** it is displayed as literal text without breaking the paragraph

### Requirement: Mermaid diagrams

Fenced `mermaid` code blocks SHALL render as diagrams.

#### Scenario: Flowchart

- **WHEN** a Mermaid block contains a flowchart
- **THEN** a diagram is displayed

#### Scenario: Invalid diagram

- **WHEN** a Mermaid block is invalid
- **THEN** the source is shown with an error indicator

#### Scenario: Oversized diagram

- **WHEN** a diagram is wider than the preview
- **THEN** it scales to fit without clipping

### Requirement: Images and links

The preview SHALL resolve relative image paths from the document folder, display images, and open external links in the system browser.

#### Scenario: Relative image

- **WHEN** a document references a relative image that exists
- **THEN** the image is displayed

#### Scenario: Missing image

- **WHEN** an image cannot be found
- **THEN** its alt text and a placeholder are shown

#### Scenario: External link

- **WHEN** the user activates an HTTP or HTTPS link
- **THEN** it opens in the default browser

### Requirement: HTML sanitization

The preview SHALL prevent scripts, event handlers, and unsafe URLs from executing.

#### Scenario: Script element

- **WHEN** a document contains a script element
- **THEN** it is not executed

#### Scenario: Unsafe URL

- **WHEN** a link uses a JavaScript URL
- **THEN** it is not followed

#### Scenario: Safe HTML

- **WHEN** a document contains safe HTML such as line breaks or keyboard tags
- **THEN** it renders

### Requirement: Read mode

The application SHALL provide a read mode that shows only the rendered document with reading typography, an outline, and reading progress.

#### Scenario: Enter read mode

- **WHEN** the user enters read mode
- **THEN** editor chrome is hidden and the document fills the content area

#### Scenario: Outline navigation

- **WHEN** the user selects a heading in the outline
- **THEN** the document scrolls to that heading

#### Scenario: Reading progress

- **WHEN** the user scrolls the document
- **THEN** reading progress updates

### Requirement: Scroll synchronization

In side-by-side mode, scrolling either the editor or the preview SHALL keep the other view at the corresponding content.

#### Scenario: Editor to preview

- **WHEN** the user scrolls the editor
- **THEN** the preview follows the corresponding block

#### Scenario: Preview to editor

- **WHEN** the user scrolls the preview
- **THEN** the editor follows the corresponding source line

#### Scenario: Long document

- **WHEN** the document is long
- **THEN** synchronization remains stable without visible jumping
