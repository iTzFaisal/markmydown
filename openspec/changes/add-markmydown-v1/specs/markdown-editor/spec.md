# Spec Delta

## Purpose

Provides a fast, keyboard-first source editor for Markdown with formatting actions, search, outline, focus, and layout controls.

## ADDED Requirements

### Requirement: Source editing

The application SHALL provide a multiline plain-text editor for Markdown with syntax highlighting, selection, undo and redo, clipboard operations, and input-method composition.

#### Scenario: Typing

- **WHEN** the user types in the editor
- **THEN** text appears without perceptible lag

#### Scenario: Undo and redo

- **WHEN** the user invokes undo
- **THEN** the last edit is reversed, and redo restores it

#### Scenario: Input-method composition

- **WHEN** the user composes text with an input method
- **THEN** committed text appears once and composition is not duplicated

### Requirement: Formatting actions

The application SHALL provide actions for bold, italic, heading, strikethrough, unordered, ordered and check lists, blockquote, code, table, link, and image, with keyboard shortcuts.

#### Scenario: Bold

- **WHEN** the user selects text and invokes bold
- **THEN** the selection is wrapped in bold markers

#### Scenario: Link

- **WHEN** the user invokes link
- **THEN** a popover accepts a URL and applies it to the selection

#### Scenario: Table

- **WHEN** the user invokes table
- **THEN** a table skeleton is inserted and the cursor is placed in the first cell

### Requirement: Smart editing behaviors

The editor SHALL continue lists on Enter, indent and outdent list items with Tab and Shift+Tab, and convert a pasted URL into a link when text is selected.

#### Scenario: Continue a list

- **WHEN** the user presses Enter at the end of a list item
- **THEN** a new item with the same marker is created

#### Scenario: Indent a list item

- **WHEN** the user presses Tab in a list item
- **THEN** the item is indented one level

#### Scenario: Paste a URL over selected text

- **WHEN** the user pastes a URL while text is selected
- **THEN** the selection becomes a link to that URL

### Requirement: Find and replace

The application SHALL support finding and replacing text in the current document, including case-sensitive and regular-expression search and replace-all.

#### Scenario: Find text

- **WHEN** the user enters a query
- **THEN** matches are highlighted and next and previous navigation works

#### Scenario: Replace all

- **WHEN** the user invokes replace-all
- **THEN** all matches are replaced and the change can be undone as one action

#### Scenario: Invalid regular expression

- **WHEN** the user enters an invalid regular expression
- **THEN** the application shows an error and makes no replacement

### Requirement: Outline and status

The application SHALL show an outline of document headings and a status bar with word count, character count, reading time, and cursor line and column.

#### Scenario: Outline navigation

- **WHEN** the document has headings
- **THEN** the outline lists them and selecting one moves the editor to it

#### Scenario: Document statistics

- **WHEN** the document changes
- **THEN** the word count and reading time update

#### Scenario: Selection statistics

- **WHEN** the user selects text
- **THEN** the status bar reports statistics for the selection

### Requirement: Focus mode

The application SHALL provide a focus mode that keeps the caret vertically centered and visually emphasizes the current paragraph.

#### Scenario: Enable focus mode

- **WHEN** the user enables focus mode
- **THEN** the caret remains centered while typing and non-current text is dimmed

#### Scenario: Disable focus mode

- **WHEN** the user disables focus mode
- **THEN** the editor returns to its normal appearance and scrolling

### Requirement: Layout modes

The application SHALL provide editor-only, side-by-side, and preview-only layouts, and SHALL keep editor and preview scroll positions synchronized in side-by-side mode.

#### Scenario: Side-by-side layout

- **WHEN** the user selects side-by-side
- **THEN** both editor and preview are visible and scroll together

#### Scenario: Preview-only layout

- **WHEN** the user selects preview-only
- **THEN** the editor is hidden and the preview fills the content area

#### Scenario: Editor-only layout

- **WHEN** the user selects editor-only
- **THEN** the preview is hidden and the editor fills the content area

### Requirement: Keyboard shortcuts

The application SHALL provide fixed keyboard shortcuts for opening the quick-open list, saving, finding, replacing, and formatting actions, and SHALL show them in an in-app reference.

#### Scenario: Quick open

- **WHEN** the user invokes quick open
- **THEN** a searchable list of workspace documents appears

#### Scenario: Save

- **WHEN** the user invokes save
- **THEN** the current document is saved immediately

#### Scenario: Shortcut reference

- **WHEN** the user opens the shortcut reference
- **THEN** the active shortcuts are listed
