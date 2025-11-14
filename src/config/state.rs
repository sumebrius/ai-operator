use bytes::Bytes;
use standardwebhooks::Webhook;
use std::{env, fs, sync::Arc};

#[derive(Clone)]
pub struct AppState {
    prompt: Bytes,
    openai_key: Arc<String>,
    webhook: Option<Arc<Webhook>>,
}

impl AppState {
    pub fn start() -> Self {
        let prompt = Bytes::from(fs::read("./prompt.txt").expect("No prompt"));
        let openai_key = Arc::new(env::var("OPENAI_API_KEY").expect("No OpenAI API Key"));
        let webhook = env::var("OPENAPI_WEBHOOK_SECRET")
            .ok()
            .map(|secret| Webhook::new(&secret).expect("Bad Webhook Key"))
            .map(Arc::new);

        Self {
            prompt,
            openai_key,
            webhook,
        }
    }

    pub fn prompt(&self) -> Bytes {
        self.prompt.clone()
    }

    pub fn openai_key(&self) -> &str {
        &self.openai_key
    }

    pub fn webhook(&self) -> Option<Arc<Webhook>> {
        self.webhook.clone()
    }
}
