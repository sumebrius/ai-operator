use bytes::Bytes;
use std::fs;

#[derive(Clone)]
pub struct AppState {
    prompt: Bytes,
    openai_key: String,
}

impl AppState {
    pub fn start() -> Self {
        let prompt = Bytes::from(fs::read("./prompt.txt").expect("No prompt"));
        let openai_key = "foobarbaz".to_string();

        Self { prompt, openai_key }
    }

    pub fn prompt(&self) -> Bytes {
        self.prompt.clone()
    }

    pub fn openai_key(&self) -> &str {
        &self.openai_key
    }
}
