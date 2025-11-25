/// Global application state
///
/// Primarily shit loaded on init, hence being in config.
use standardwebhooks::Webhook;
use std::{env, fs, sync::Arc};

use crate::openai::api::Voice;

#[derive(Clone)]
pub struct AppState {
    prompt: Arc<String>,
    voice: Arc<Voice>,
    openai_key: Arc<String>,
    webhook: Option<Arc<Webhook>>,
    sip_realm: Arc<String>,
}

impl AppState {
    /// Initialise application state
    pub fn init() -> Self {
        info!("Initialising state");
        let prompt = Arc::new(fs::read_to_string("./prompt.txt").expect("No prompt"));
        let voice = Arc::new(
            env::var("VOICE")
                .unwrap_or_default()
                .try_into()
                .unwrap_or_default(),
        );
        let openai_key = Arc::new(env::var("OPENAI_API_KEY").expect("No OpenAI API Key"));
        let webhook = env::var("OPENAI_WEBHOOK_SECRET")
            .ok()
            .map(|secret| Webhook::new(&secret).expect("Bad Webhook Key"))
            .map(Arc::new);
        let sip_realm = Arc::new(env::var("SIP_REALM").expect("No SIP realm"));

        info!("State initialised");
        if webhook.is_none() {
            warn!("No webhook validation!");
        }

        Self {
            prompt,
            voice,
            openai_key,
            webhook,
            sip_realm,
        }
    }

    /// Get prompt for operator model
    pub fn prompt(&self) -> Arc<String> {
        self.prompt.clone()
    }

    /// Get OpenAI API Key
    pub fn openai_key(&self) -> Arc<String> {
        self.openai_key.clone()
    }

    /// Get webhook validator
    /// This is pre-initialised with the key
    pub fn webhook(&self) -> Option<Arc<Webhook>> {
        self.webhook.clone()
    }

    /// Get SIP realm for transfers
    pub fn sip_realm(&self) -> Arc<String> {
        self.sip_realm.clone()
    }

    pub fn voice(&self) -> Voice {
        *self.voice
    }
}
