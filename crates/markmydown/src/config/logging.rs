//! Local diagnostics logging. Document content is never logged.
//!
//! The log is a plain append-only text file. Records contain a UTC timestamp,
//! level, target, and message. Callers must only pass paths, document counts,
//! and error descriptions — never document text.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{Level, LevelFilter, Log, Metadata, Record};

use super::paths::AppPaths;

/// A minimal file logger. It intentionally avoids timestamps that require a
/// locale database, so it behaves the same on every supported platform.
struct FileLogger {
    file: Mutex<File>,
}

impl FileLogger {
    fn new(file: File) -> Self {
        Self {
            file: Mutex::new(file),
        }
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let line = format_record(record.level(), record.target(), &record.args().to_string());

        if let Ok(mut file) = self.file.lock() {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

/// Installs the process-wide logger and writes one startup record.
///
/// A second call is harmless: the existing logger is kept and the startup
/// record is appended directly to this process's log file.
pub fn init() -> std::io::Result<AppPaths> {
    let paths = AppPaths::discover().map_err(std::io::Error::other)?;
    init_with_paths(&paths)?;
    Ok(paths)
}

/// Installs the logger using explicit paths. Useful for tests and portable
/// builds.
pub fn init_with_paths(paths: &AppPaths) -> std::io::Result<()> {
    paths.ensure()?;

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.log_file())?;

    // `set_boxed_logger` can only succeed once. If another component already
    // installed a logger, keep it; the startup record is written directly so
    // the file always records this launch.
    let _ = log::set_boxed_logger(Box::new(FileLogger::new(file)));
    log::set_max_level(LevelFilter::Info);

    write_startup_record(paths)
}

/// Writes the startup line without touching the global logger. Kept separate
/// so tests can verify the record deterministically.
pub(crate) fn write_startup_record(paths: &AppPaths) -> std::io::Result<()> {
    append_record(
        &paths.log_file(),
        &format!(
            "starting version={} os={} arch={} config={} data={}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            paths.config_dir().display(),
            paths.data_dir().display()
        ),
    )
}

/// Appends one already-formatted record to the log file.
fn append_record(path: &std::path::Path, message: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(format_record(Level::Info, "markmydown", message).as_bytes())?;
    file.flush()
}

fn format_record(level: Level, target: &str, message: &str) -> String {
    format!(
        "{} {:<5} {}: {}\n",
        timestamp(),
        level.as_str(),
        target,
        message.replace('\n', " ")
    )
}

fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    format!("{seconds}")
}

/// Returns the log folder so the UI can reveal it in the system file manager.
pub fn log_directory(paths: &AppPaths) -> std::path::PathBuf {
    paths.log_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_record_is_written_to_the_expected_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::with_base(tmp.path());
        paths.ensure().expect("create directories");

        write_startup_record(&paths).expect("write startup record");

        let contents = std::fs::read_to_string(paths.log_file()).expect("read log");
        assert!(contents.contains("markmydown: starting version="));
        assert!(contents.contains(std::env::consts::OS));
        assert!(contents.contains("INFO"));
    }

    #[test]
    fn records_do_not_include_document_content() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::with_base(tmp.path());
        paths.ensure().expect("create directories");
        let secret = "# private draft\n\nDO NOT LOG THIS";
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(paths.log_file())
            .expect("open log");
        let logger = FileLogger::new(file);

        logger.log(
            &Record::builder()
                .args(format_args!(
                    "opened document path=/tmp/example.md bytes={}",
                    secret.len()
                ))
                .level(Level::Info)
                .target("markmydown")
                .build(),
        );

        let contents = std::fs::read_to_string(paths.log_file()).expect("read log");
        assert!(!contents.contains("private draft"));
        assert!(!contents.contains("DO NOT LOG THIS"));
        assert!(contents.contains("opened document"));
    }
}
