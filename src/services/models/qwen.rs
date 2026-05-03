use agentx::{ModelOptions, StreamingChatModel};

use crate::services::models::Model;

pub struct Qwen {
    options: ModelOptions,
}

impl agentx::Model for Qwen {
    fn options(&self) -> &ModelOptions {
        &self.options
    }
}

impl StreamingChatModel for Qwen {}

impl Model for Qwen {
    fn new(options: ModelOptions) -> Self {
        Self { options }
    }

    fn name() -> &'static str {
        "qwen"
    }
}
