use agentx::{ModelOptions, StreamingChatModel};

use crate::services::models::Model;

pub struct Qwen3VL {
    options: ModelOptions,
}

impl agentx::Model for Qwen3VL {
    fn options(&self) -> &ModelOptions {
        &self.options
    }
}

impl StreamingChatModel for Qwen3VL {}

impl Model for Qwen3VL {
    fn new(options: ModelOptions) -> Self {
        Self { options }
    }

    fn name() -> &'static str {
        "qwen3-vl"
    }
}
