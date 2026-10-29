fn main() -> iced::Result {
    if let Err(error) = markmydown::config::logging::init() {
        eprintln!("markmydown: could not initialize logging: {error}");
    }

    let documents = markmydown::config::launch::document_paths(std::env::args_os());
    markmydown::app::run_with_paths(documents)
}
