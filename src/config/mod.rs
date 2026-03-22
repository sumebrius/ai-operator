mod state;

pub use state::AppState;

// pub const API_ROOT: &str = "http://localhost:3000";
pub const API_ROOT: &str = "https://api.openai.com/v1/realtime/calls";
pub const WS_URI: &str = "wss://api.openai.com/v1/realtime";

pub const DEFAULT_MODEL: &str = "gpt-realtime-1.5";
