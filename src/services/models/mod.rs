pub mod qwen3;
pub mod qwen3vl;

pub use qwen3::Qwen3;
pub use qwen3vl::Qwen3VL;

use agentx::{Completion, ModelOptions, OpenAIModelOptions, Stream, StreamingChatModel};
use anyhow::anyhow;

use crate::{
    entities::{
        config::{ModelConfig, ServiceConfig},
        message::Message,
    },
    services::{Inject, Service},
};

pub trait Model: StreamingChatModel + Inject {
    fn new(options: ModelOptions) -> Self;

    fn name() -> &'static str;
}

impl<T: Model> Inject for T {
    fn new(config: &ServiceConfig) -> Self {
        let name = Self::name();
        let ModelConfig {
            model,
            base_url,
            api_key,
        } = &config
            .models
            .get(name)
            .ok_or_else(|| anyhow!("missing model configuration '{}'", name))
            .unwrap();
        <Self as Model>::new(
            OpenAIModelOptions::new()
                .model(model)
                .base_url(base_url)
                .api_key(api_key)
                .into(),
        )
    }
}

impl<M: Model> Service<M> {
    pub async fn completion(&self, message: Message) -> anyhow::Result<Completion> {
        self.0.completion(&message.into()).await
    }

    pub async fn text_stream(&self, message: Message) -> anyhow::Result<Stream<String>> {
        self.0.text_stream(&message.into()).await
    }
}
