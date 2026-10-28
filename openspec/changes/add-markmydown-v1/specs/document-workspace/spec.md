# Spec Delta

## Purpose

Lets users open and manage a local folder of Markdown documents, with tabs, autosave, session restore, and safe handling of external changes.

## ADDED Requirements

### Requirement: Open a local workspace folder

The application SHALL let the user select a local folder as the workspace and SHALL display supported Markdown documents and subfolders in a tree.

#### Scenario: Open a folder

- **WHEN** the user selects a folder
- **THEN** the application displays its Markdown documents and subfolders in a tree and remembers the folder for the next launch

#### Scenario: Empty folder

- **WHEN** the selected folder contains no supported documents
- **THEN** the application shows an empty state with actions to create a document or choose another folder

#### Scenario: Unsupported files

- **WHEN** the folder contains files that are not supported Markdown documents
- **THEN** the application does not present them as editable documents

### Requirement: Manage files and folders

The application SHALL allow the user to create, rename, delete, and move files and folders within the workspace, and SHALL make deletions recoverable.

#### Scenario: Create a document

- **WHEN** the user creates a document
- **THEN** the document is created on disk, appears in the tree, and opens in a tab

#### Scenario: Rename a document

- **WHEN** the user renames a document
- **THEN** the file on disk, the tree label, and the tab title update

#### Scenario: Delete a document

- **WHEN** the user deletes a document
- **THEN** it is moved to the operating system trash and removed from the tree

#### Scenario: Move a document

- **WHEN** the user moves a document into another folder
- **THEN** the file moves on disk and the tree reflects its new location

### Requirement: Tabs and session restore

The application SHALL support multiple open documents in tabs and SHALL restore the previous session on launch.

#### Scenario: Multiple tabs

- **WHEN** the user opens several documents
- **THEN** each appears in a tab with a dirty indicator when it has unsaved changes

#### Scenario: Restore a session

- **WHEN** the application restarts
- **THEN** previously open tabs, the active tab, cursor position, and scroll position are restored

#### Scenario: Recover after a crash

- **WHEN** the application exits unexpectedly with unsaved changes
- **THEN** on the next launch it offers to recover them

### Requirement: Autosave and explicit save

The application SHALL autosave edits after a short idle period and SHALL provide an explicit save action.

#### Scenario: Autosave

- **WHEN** the user stops typing for a short interval
- **THEN** the current document is written to disk without a dialog

#### Scenario: Explicit save

- **WHEN** the user invokes Save
- **THEN** the current document is written immediately

#### Scenario: Save failure

- **WHEN** a write fails
- **THEN** the document remains open, the application shows an error, and the document is not marked saved

### Requirement: External change handling

The application SHALL detect changes made to open documents by other programs and SHALL NOT silently overwrite them.

#### Scenario: Clean file changed on disk

- **WHEN** an open document changes on disk and has no unsaved edits
- **THEN** the application reloads it

#### Scenario: Dirty file changed on disk

- **WHEN** an open document changes on disk and has unsaved edits
- **THEN** the application offers to reload, keep local edits, or compare

#### Scenario: Open file deleted on disk

- **WHEN** an open document is deleted on disk
- **THEN** its tab is marked missing and the user can save it again or close it

### Requirement: Supported documents and encoding

The application SHALL open `.md`, `.markdown`, `.mdown`, and `.txt` files as UTF-8 by default, preserve line endings, and report undecodable files without corrupting them.

#### Scenario: UTF-8 document

- **WHEN** a UTF-8 document is opened
- **THEN** its content is displayed correctly

#### Scenario: CRLF line endings

- **WHEN** a document uses CRLF line endings
- **THEN** saving preserves CRLF

#### Scenario: Undecodable document

- **WHEN** a document cannot be decoded
- **THEN** the application shows an error and leaves the file unchanged

### Requirement: Import documents

The application SHALL import Markdown documents and SHALL convert HTML documents to Markdown when imported.

#### Scenario: Import Markdown

- **WHEN** the user imports a Markdown file
- **THEN** it is added to the workspace and opened

#### Scenario: Import HTML

- **WHEN** the user imports an HTML file
- **THEN** it is converted to Markdown, added to the workspace, and opened

#### Scenario: Import failure

- **WHEN** conversion fails
- **THEN** the application reports the error and leaves the workspace unchanged

### Requirement: Front matter

The application SHALL parse YAML front matter, SHALL hide it from the rendered body, and SHALL make its metadata available to preview and export.

#### Scenario: Valid front matter

- **WHEN** a document begins with valid YAML front matter
- **THEN** the body excludes it and its title, date, and tags are available to export

#### Scenario: Invalid front matter

- **WHEN** front matter is malformed
- **THEN** the application shows a validation message and treats the block as ordinary text
