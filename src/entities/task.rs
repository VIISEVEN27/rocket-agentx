use crate::{
    entities::{datetime::DateTime, message::Message},
    services::{models::Model, Service},
};
use agentx::Completion;
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
    pub message: Message,
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
            message,
            completion: None,
            err_msg: None,
            create_time: DateTime::local(),
            finish_time: None,
        }
    }

    pub async fn execute<T: Model>(&mut self, model: &Service<T>) -> () {
        match model.completion(self.message.clone()).await {
            Ok(completion) => {
                self.status = Status::Finished;
                self.completion = Some(completion);
                self.finish_time = Some(DateTime::local());
            }
            Err(err) => {
                self.status = Status::Failed;
                self.err_msg = Some(err.to_string());
            }
        }
    }
}
