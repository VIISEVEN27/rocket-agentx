use crate::entities::message::Message;
use crate::entities::response::Response;
use crate::services::model::Model;
use crate::services::Service;
use agentx::Completion;
use rocket::http::Status;
use rocket::post;
use rocket::response::status::{self};
use rocket::response::stream::TextStream;
use rocket::serde::json::Json;

#[post("/completion?<model>", data = "<message>")]
pub async fn completion(
    model: Option<String>,
    message: Json<Message>,
    service: &Service<Model>,
) -> Json<Response<Completion>> {
    Response::invoke(async {
        let completion = service.completion(model, message.into_inner()).await?;
        Ok(completion)
    })
    .await
    .into()
}

#[post("/stream?<model>", data = "<message>")]
pub async fn stream(
    model: Option<String>,
    message: Json<Message>,
    service: &Service<Model>,
) -> Result<TextStream![String], status::Custom<String>> {
    service
        .text_stream(model, message.into_inner())
        .await
        .map(|stream| TextStream::from(stream.into_inner()))
        .map_err(|err| {
            eprint!("Failed to streaming chat: {:?}", err);
            status::Custom(Status::InternalServerError, format!("{:#}", err))
        })
}
