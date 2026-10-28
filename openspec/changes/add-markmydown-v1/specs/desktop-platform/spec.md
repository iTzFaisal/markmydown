# Spec Delta

## Purpose

Delivers markmydown as a native, offline-capable desktop application on Windows, macOS, and Linux with installers, file associations, and updates.

## ADDED Requirements

### Requirement: Native cross-platform application

The application SHALL run as a native desktop application on Windows 10 and 11, macOS 12 and later, and Linux, without requiring a browser or webview.

#### Scenario: Launch the application

- **WHEN** the user launches the installed application
- **THEN** a native window opens

#### Scenario: Work offline

- **WHEN** no network is available
- **THEN** editing, preview, and export continue to work

#### Scenario: Install on each platform

- **WHEN** the application is installed on each supported operating system
- **THEN** it starts and can open a folder

### Requirement: Installable packages

The project SHALL produce installable packages for each platform: a macOS disk image or application bundle, a Windows MSI or NSIS installer, and a Linux AppImage or Debian package.

#### Scenario: Install a package

- **WHEN** the user installs a package
- **THEN** the application appears in the operating system's application list and launches

#### Scenario: Uninstall the application

- **WHEN** the user uninstalls the application
- **THEN** application binaries are removed and user documents are untouched

### Requirement: File associations and open-with

The application SHALL open `.md` and `.markdown` files from the operating system file manager and SHALL accept file paths on the command line.

#### Scenario: Open from the file manager

- **WHEN** the user opens a Markdown file from the file manager
- **THEN** the application opens that file

#### Scenario: Open from the command line

- **WHEN** the user runs the application with a file path
- **THEN** the file opens

#### Scenario: Open multiple files

- **WHEN** multiple file paths are passed
- **THEN** each file opens in a tab

### Requirement: Updates

The application SHALL check for updates and SHALL let the user install an available update.

#### Scenario: Update available

- **WHEN** a newer signed release exists
- **THEN** the application notifies the user

#### Scenario: Install an update

- **WHEN** the user accepts an update
- **THEN** the update downloads, verifies, and restarts the application

#### Scenario: Update check offline

- **WHEN** the update check cannot reach the network
- **THEN** the application continues normally and does not block startup

### Requirement: Security and privacy

The application SHALL not execute scripts from documents and SHALL not send document content over the network without explicit user action.

#### Scenario: No network transmission while editing

- **WHEN** the user edits a document
- **THEN** its content is not transmitted

#### Scenario: Script in a document

- **WHEN** a document contains a script
- **THEN** it is not executed

#### Scenario: Update metadata

- **WHEN** the application checks for updates
- **THEN** only version metadata is requested

### Requirement: Crash recovery

The application SHALL preserve unsaved work across unexpected exits.

#### Scenario: Recover after a crash

- **WHEN** the application exits unexpectedly
- **THEN** on the next launch it offers to recover unsaved documents

#### Scenario: Accept recovery

- **WHEN** the user accepts recovery
- **THEN** the recovered content opens in a tab marked unsaved

#### Scenario: Decline recovery

- **WHEN** the user declines recovery
- **THEN** recovery data is discarded after confirmation

### Requirement: Diagnostics

The application SHALL write local error logs and SHALL let the user open the log location.

#### Scenario: Log an error

- **WHEN** an operation fails
- **THEN** a local log entry is written

#### Scenario: Open the log folder

- **WHEN** the user chooses to open logs
- **THEN** the operating system file manager opens the log folder

#### Scenario: Keep logs private

- **WHEN** logs are written
- **THEN** document content is excluded by default
