# Spec Delta

## Purpose

Lets users make the editor and preview personal and comfortable through themes, typography, and persisted preferences.

## ADDED Requirements

### Requirement: Theme selection

The application SHALL offer light, dark, and system themes and SHALL apply the selected theme to the editor, preview, and exported output.

#### Scenario: Select light or dark

- **WHEN** the user selects a theme
- **THEN** the interface updates immediately

#### Scenario: Follow the system theme

- **WHEN** the operating system theme changes while the system option is selected
- **THEN** the application follows it

#### Scenario: Theme in exports

- **WHEN** the user exports a document
- **THEN** the active theme's colors and typography are used

### Requirement: Typography

The application SHALL let users choose font family, font size, line height, and maximum text width for the editor and preview independently.

#### Scenario: Change the editor font

- **WHEN** the user changes the editor font
- **THEN** the editor updates without changing the preview

#### Scenario: Change the preview font

- **WHEN** the user changes the preview font
- **THEN** the preview updates without changing the editor

#### Scenario: Reset typography

- **WHEN** the user resets typography
- **THEN** the default values are restored

### Requirement: Settings persistence

Settings SHALL persist locally and SHALL survive restarts and updates.

#### Scenario: Restore settings after restart

- **WHEN** the application restarts
- **THEN** theme and typography choices are restored

#### Scenario: Unreadable settings

- **WHEN** settings cannot be read
- **THEN** the application starts with defaults and preserves the unreadable file for diagnosis

#### Scenario: Reset settings

- **WHEN** the user resets settings
- **THEN** defaults are restored after confirmation

### Requirement: Theme preview

The application SHALL let users preview a theme before applying it.

#### Scenario: Preview a theme

- **WHEN** the user focuses or hovers over a theme
- **THEN** the interface previews that theme

#### Scenario: Apply a theme

- **WHEN** the user confirms a theme
- **THEN** the theme is applied and persisted

#### Scenario: Cancel a preview

- **WHEN** the user leaves the theme picker without confirming
- **THEN** the previous theme is restored

### Requirement: Welcome content

The application SHALL provide a welcome document that demonstrates Markdown, code, block math, diagrams, and links.

#### Scenario: First launch without a workspace

- **WHEN** the application opens with no workspace
- **THEN** the welcome document is shown or offered

#### Scenario: Welcome offline

- **WHEN** no network is available
- **THEN** the welcome document still renders

### Requirement: Cheat sheet

The application SHALL provide an in-app Markdown cheat sheet and keyboard shortcut reference.

#### Scenario: Open the cheat sheet

- **WHEN** the user opens the cheat sheet
- **THEN** Markdown syntax and shortcuts are shown

#### Scenario: Close the cheat sheet

- **WHEN** the user closes the cheat sheet
- **THEN** the previous document is shown
