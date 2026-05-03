use crate::entities::message::Message;
use crate::entities::response::Response;
use crate::services::models::Qwen;
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
    qwen: &Service<Qwen>,
) -> Json<Response<Completion>> {
    Response::invoke(async {
        let message = message.into_inner();
        qwen.completion(&message.into()).await
    })
    .await
    .into()
}

#[post("/stream", data = "<message>")]
pub async fn stream(
    message: Json<Message>,
    qwen: &Service<Qwen>,
) -> Result<TextStream![String], status::Custom<String>> {
    let message = message.into_inner();
    qwen.text_stream(&message.into())
        .await
        .map(|stream| TextStream::from(stream.into_inner()))
        .map_err(|err| {
            eprint!("Failed to streaming chat: {:?}", err);
            status::Custom(Status::InternalServerError, format!("{:#}", err))
        })
}
