use agentx::{ModelOptions, StreamingChatModel};

use crate::services::models::Model;

pub struct Qwen3 {
    options: ModelOptions,
}

impl agentx::Model for Qwen3 {
    fn options(&self) -> &ModelOptions {
        &self.options
    }
}

impl StreamingChatModel for Qwen3 {}

impl Model for Qwen3 {
    fn new(options: ModelOptions) -> Self {
        Self { options }
    }

    fn name() -> &'static str {
        "qwen3"
    }
}
