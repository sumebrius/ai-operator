mod state;

pub use state::AppState;

pub const API_ROOT: &str = "https://api.openai.com/v1/realtime/calls";
pub const WS_URI: &str = "wss://api.openai.com/v1/realtime";
