fn main() -> iced::Result {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if args
        .iter()
        .skip(1)
        .any(|arg| arg == "--version" || arg == "-V")
    {
        println!("markmydown {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if let Err(error) = markmydown::config::logging::init() {
        eprintln!("markmydown: could not initialize logging: {error}");
    }

    let documents = markmydown::config::launch::document_paths(args);
    markmydown::app::run_with_paths(documents)
}
