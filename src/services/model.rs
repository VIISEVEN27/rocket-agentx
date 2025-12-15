use agentx::{Completion, ModelOptions, OpenAIModelOptions, Stream, StreamingChatModel};

use crate::{
    entities::{
        config::{ModelConfig, ServiceConfig},
        message::Message,
    },
    services::{Inject, Service},
};

pub struct Model {
    options: ModelOptions,
}

impl agentx::Model for Model {
    fn options(&self) -> &ModelOptions {
        &self.options
    }
}

impl StreamingChatModel for Model {}

impl Inject for Model {
    fn new(config: &ServiceConfig) -> Self {
        let ModelConfig {
            model,
            base_url,
            api_key,
        } = &config.model;
        Self {
            options: OpenAIModelOptions::new()
                .model(model)
                .base_url(base_url)
                .api_key(api_key)
                .into(),
        }
    }
}

impl<M: StreamingChatModel + Inject> Service<M> {
    pub async fn completion<T: AsRef<str>>(
        &self,
        model: Option<T>,
        message: Message,
    ) -> anyhow::Result<Completion> {
        if let Some(model) = model {
            self.0
                .completion_with(
                    &message.into(),
                    OpenAIModelOptions::new().model(model).into(),
                )
                .await
        } else {
            self.0.completion(&message.into()).await
        }
    }

    pub async fn text_stream<T: AsRef<str>>(
        &self,
        model: Option<T>,
        message: Message,
    ) -> anyhow::Result<Stream<String>> {
        if let Some(model) = model {
            self.0
                .text_stream_with(
                    &message.into(),
                    OpenAIModelOptions::new().model(model).into(),
                )
                .await
        } else {
            self.0.text_stream(&message.into()).await
        }
    }
}
