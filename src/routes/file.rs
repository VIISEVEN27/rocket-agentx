use crate::entities::oss::ObjectMeta;
use crate::entities::response::Response;
use crate::services::oss::OSS;
use crate::services::Service;
use bytes::Bytes;
use futures::Stream;
use rocket::http::{Header, Status};
use rocket::response::stream::ByteStream;
use rocket::response::{status, Responder};
use rocket::serde::json::Json;
use rocket::{get, post, Data, Request};

#[post("/upload", data = "<data>")]
pub async fn upload(
    data: Data<'_>,
    meta: ObjectMeta,
    oss: &Service<OSS>,
) -> Json<Response<String>> {
    Response::invoke(async { oss.put_object(data, meta).await })
        .await
        .into()
}

pub enum FileResponder<S: Stream<Item = Bytes> + Send> {
    Ok(S, ObjectMeta),
    Err(Status, anyhow::Error),
}

impl<'r, S: Stream<Item = Bytes> + Send + 'r> Responder<'r, 'r> for FileResponder<S> {
    fn respond_to(self, request: &'r Request<'_>) -> rocket::response::Result<'r> {
        match self {
            Self::Ok(stream, meta) => {
                let mut builder =
                    rocket::Response::build_from(ByteStream::from(stream).respond_to(request)?);
                if let Ok(content_type) = meta.content_type() {
                    builder.header(content_type);
                }
                builder.header(Header::new(
                    "Content-Length",
                    meta.content_length.to_string(),
                ));
                builder.ok()
            }
            Self::Err(status, err) => rocket::Response::build_from(
                status::Custom(status, format!("{:#}", err)).respond_to(request)?,
            )
            .ok(),
        }
    }
}

#[get("/download/<name>")]
pub async fn download(
    name: &str,
    oss: &Service<OSS>,
) -> FileResponder<impl Stream<Item = Bytes> + Send> {
    match oss.get_object(name).await {
        Ok((stream, meta)) => FileResponder::Ok(stream, meta),
        Err(err) => {
            eprint!("Failed to download file '{}': {:?}", name, err);
            FileResponder::Err(Status::InternalServerError, err)
        }
    }
}
