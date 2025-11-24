use standardwebhooks::Webhook;
use std::{env, fs, sync::Arc};

#[derive(Clone)]
pub struct AppState {
    prompt: Arc<String>,
    openai_key: Arc<String>,
    webhook: Option<Arc<Webhook>>,
    sip_realm: Arc<String>,
    transcribe_caller: bool,
}

impl AppState {
    pub fn start() -> Self {
        info!("Initialising state");
        let prompt = Arc::new(fs::read_to_string("./prompt.txt").expect("No prompt"));
        let openai_key = Arc::new(env::var("OPENAI_API_KEY").expect("No OpenAI API Key"));
        let webhook = env::var("OPENAPI_WEBHOOK_SECRET")
            .ok()
            .map(|secret| Webhook::new(&secret).expect("Bad Webhook Key"))
            .map(Arc::new);
        let sip_realm = Arc::new(env::var("SIP_REALM").expect("No SIP realm"));
        let transcribe_caller = env::var("TRANSCRIBE_CALLER").is_ok();
        info!("State initialised");
        if webhook.is_none() {
            warn!("No webhook validation!");
        }

        Self {
            prompt,
            openai_key,
            webhook,
            sip_realm,
            transcribe_caller,
        }
    }

    pub fn prompt(&self) -> Arc<String> {
        self.prompt.clone()
    }

    pub fn openai_key(&self) -> Arc<String> {
        self.openai_key.clone()
    }

    pub fn webhook(&self) -> Option<Arc<Webhook>> {
        self.webhook.clone()
    }

    pub fn sip_realm(&self) -> Arc<String> {
        self.sip_realm.clone()
    }

    pub fn transcribe_caller(&self) -> bool {
        self.transcribe_caller
    }
}
