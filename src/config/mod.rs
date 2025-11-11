mod logging;
mod state;

pub use state::AppState;

pub(super) fn configure() {
    logging::configure_logging();
}

pub const API_ROOT: &str = "http://localhost:3000";
// pub const API_ROOT: &str = "https://api.openai.com/v1/realtime/calls";
