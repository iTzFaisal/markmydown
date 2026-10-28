# Spec Delta

## Purpose

Turns the current Markdown document into portable Markdown, standalone HTML, or PDF, and supports copying and printing.

## ADDED Requirements

### Requirement: Export Markdown

The application SHALL export the current document as a Markdown file with its source unchanged.

#### Scenario: Export a document

- **WHEN** the user exports Markdown
- **THEN** a save dialog accepts a destination and the file is written

#### Scenario: Cancel an export

- **WHEN** the user cancels the save dialog
- **THEN** no file is written

#### Scenario: Missing extension

- **WHEN** the user omits a file extension
- **THEN** the application appends `.md`

### Requirement: Export standalone HTML

The application SHALL export a standalone HTML document with embedded styling, code highlighting, block math, and diagrams.

#### Scenario: Self-contained output

- **WHEN** the user exports HTML
- **THEN** the file opens correctly without the application or a network connection

#### Scenario: Local images

- **WHEN** the document references local images
- **THEN** the export includes them or copies an assets folder and references it

#### Scenario: Theme fidelity

- **WHEN** a theme is active
- **THEN** the exported HTML uses the same colors and typography

### Requirement: Export PDF

The application SHALL export a paginated PDF with readable typography, code highlighting, block math, and diagrams.

#### Scenario: Generate a PDF

- **WHEN** the user exports PDF
- **THEN** a PDF is produced using the configured page size and margins

#### Scenario: Math and diagrams in PDF

- **WHEN** the document contains block math or Mermaid diagrams
- **THEN** they appear in the PDF

#### Scenario: Long document

- **WHEN** the document spans many pages
- **THEN** content is not clipped and code blocks and tables are not split where avoidable

### Requirement: Copy as HTML

The application SHALL copy the rendered document to the clipboard as HTML and as plain text.

#### Scenario: Copy rich content

- **WHEN** the user copies the document as HTML
- **THEN** pasting into a rich-text editor preserves headings, lists, links, and emphasis

#### Scenario: Clipboard fallback

- **WHEN** HTML clipboard data is unavailable
- **THEN** plain text is copied

### Requirement: Print

The application SHALL print the rendered document with the same layout as PDF export.

#### Scenario: Open the print dialog

- **WHEN** the user invokes print
- **THEN** the system print dialog opens with the document

#### Scenario: Cancel printing

- **WHEN** the user cancels the print dialog
- **THEN** no print job is created

### Requirement: Export failure handling

The application SHALL report export failures without losing the document or closing the application.

#### Scenario: Destination cannot be written

- **WHEN** the destination cannot be written
- **THEN** the application shows an error and keeps the document open

#### Scenario: Invalid destination

- **WHEN** the destination is not writable
- **THEN** the application reports it and offers another location

### Requirement: Output consistency

The application SHALL use the same rendered content for preview, HTML, PDF, and print.

#### Scenario: Same content everywhere

- **WHEN** the same document is previewed and exported
- **THEN** headings, lists, links, code, math, and diagrams match

#### Scenario: Theme change

- **WHEN** the theme changes
- **THEN** subsequent exports use the new theme
