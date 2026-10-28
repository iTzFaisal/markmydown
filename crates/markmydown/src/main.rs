fn main() -> iced::Result {
    if let Err(error) = markmydown::config::logging::init() {
        eprintln!("markmydown: could not initialize logging: {error}");
    }

    markmydown::app::run()
}
