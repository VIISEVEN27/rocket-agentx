use crate::entities::{datetime::DateTime, message::Message};
use agentx::{Completion, Prompt};
use chrono::Local;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Running,
    Finished,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Task {
    pub id: String,
    pub status: Status,
    pub prompt: Prompt,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub completion: Option<Completion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub err_msg: Option<String>,
    pub create_time: DateTime<Local>,
    pub finish_time: Option<DateTime<Local>>,
}

impl Task {
    pub fn create(message: Message) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            status: Status::Pending,
            prompt: message.into(),
            completion: None,
            err_msg: None,
            create_time: DateTime::local(),
            finish_time: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ser_task() {
        let message = Message {
            role: None,
            text: Some("你是谁".to_owned()),
            images: None,
            videos: None,
            context: None,
        };
        let task = Task::create(message);
        let json = serde_json::to_string(&task).unwrap();
        println!("{}", json);
    }
}
