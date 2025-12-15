use std::str::FromStr;

use anyhow::anyhow;
use rocket::{
    http::{ContentType, Status},
    request::{FromRequest, Outcome},
    Request,
};

#[derive(Debug)]
pub struct ObjectMeta {
    pub content_type: String,
    pub content_length: u64,
}

impl ObjectMeta {
    pub fn content_type(&self) -> anyhow::Result<ContentType> {
        ContentType::from_str(&self.content_type).map_err(|err| {
            anyhow!(
                "Invalid value 'Content-Type: {}': {}",
                self.content_type,
                err
            )
        })
    }

    pub fn extension(&self) -> anyhow::Result<String> {
        self.content_type()?
            .extension()
            .map(ToString::to_string)
            .ok_or_else(|| {
                anyhow!(
                    "Unknown extension from 'Content-Type: {}'",
                    self.content_type
                )
            })
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ObjectMeta {
    type Error = anyhow::Error;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let headers = request.headers();
        let content_type = match headers.get_one("Content-Type") {
            Some(content_type) => content_type.to_owned(),
            None => {
                return Outcome::Error((
                    Status::BadRequest,
                    anyhow!("Missing request header 'Content-Type'"),
                ))
            }
        };
        let content_length = match headers.get_one("Content-Length") {
            Some(s) => match s.parse() {
                Ok(content_length) => content_length,
                Err(_) => {
                    return Outcome::Error((
                        Status::BadRequest,
                        anyhow!("Invalid request header 'Content-Length': {}", s),
                    ))
                }
            },
            None => {
                return Outcome::Error((
                    Status::BadRequest,
                    anyhow!("Missing request header 'Content-Length'"),
                ))
            }
        };
        Outcome::Success(ObjectMeta {
            content_type,
            content_length,
        })
    }
}
