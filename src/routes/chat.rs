use crate::entities::message::Message;
use crate::entities::response::Response;
use crate::services::models::{Qwen3, Qwen3VL};
use crate::services::Service;
use agentx::Completion;
use rocket::http::Status;
use rocket::post;
use rocket::response::status;
use rocket::response::stream::TextStream;
use rocket::serde::json::Json;

#[post("/completion", data = "<message>")]
pub async fn completion(
    message: Json<Message>,
    qwen3: &Service<Qwen3>,
    qwen3vl: &Service<Qwen3VL>,
) -> Json<Response<Completion>> {
    Response::invoke(async {
        let completion = if message.only_text() {
            qwen3.completion(message.into_inner()).await?
        } else {
            qwen3vl.completion(message.into_inner()).await?
        };
        Ok(completion)
    })
    .await
    .into()
}

#[post("/stream", data = "<message>")]
pub async fn stream(
    message: Json<Message>,
    qwen3: &Service<Qwen3>,
    qwen3vl: &Service<Qwen3VL>,
) -> Result<TextStream![String], status::Custom<String>> {
    let result = if message.only_text() {
        qwen3.text_stream(message.into_inner()).await
    } else {
        qwen3vl.text_stream(message.into_inner()).await
    };
    result
        .map(|stream| TextStream::from(stream.into_inner()))
        .map_err(|err| {
            eprint!("Failed to streaming chat: {:?}", err);
            status::Custom(Status::InternalServerError, format!("{:#}", err))
        })
}
