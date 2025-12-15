use agentx::{message::Media, Prompt, Role};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
#[serde(untagged)]
pub enum Video {
    Url(String),
    Images(Vec<String>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub role: Option<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub images: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub videos: Option<Vec<Video>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub context: Option<Vec<Message>>,
}

impl From<Message> for agentx::Message {
    fn from(message: Message) -> Self {
        let Message {
            role,
            text,
            images,
            videos,
            ..
        } = message;
        if images.is_some() || videos.is_some() {
            let content: Vec<Media> = if let Some(images) = images {
                images.into_iter().map(Media::ImageUrl).collect()
            } else if let Some(videos) = videos {
                videos
                    .into_iter()
                    .map(|video| match video {
                        Video::Url(url) => Media::VideoUrl(url),
                        Video::Images(urls) => Media::Video(urls),
                    })
                    .collect()
            } else {
                unreachable!();
            };
            agentx::Message::media(role.unwrap_or(Role::User))
                .content(content)
                .into()
        } else {
            agentx::Message::text(role.unwrap_or(Role::User), text.unwrap())
        }
    }
}

impl From<Message> for Prompt {
    fn from(mut message: Message) -> Self {
        let mut messages: Vec<agentx::Message> = message
            .context
            .take()
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();
        messages.push(message.into());
        messages.into()
    }
}
