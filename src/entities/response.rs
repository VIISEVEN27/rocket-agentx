use std::{fmt::Display, future::Future};

use rocket::{http::Status, response::status};
use serde::Serialize;

#[derive(Serialize)]
pub struct Response<T> {
    success: bool,
    msg: String,
    data: Option<T>,
}

impl<T> Response<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            msg: "成功".to_string(),
            data: Some(data),
        }
    }

    pub fn error<M: Display>(msg: M) -> Self {
        Self {
            success: false,
            msg: msg.to_string(),
            data: None,
        }
    }

    pub async fn invoke<F>(future: F) -> Self
    where
        F: Future<Output = anyhow::Result<T>>,
    {
        match future.await {
            Ok(data) => Self::ok(data),
            Err(err) => Self::error(format!("{:#}", err)),
        }
    }
}

impl<T> From<Response<T>> for Result<T, status::Custom<String>> {
    fn from(response: Response<T>) -> Self {
        if response.success {
            Ok(response.data.unwrap())
        } else {
            eprintln!("Invoke error: {}", response.msg);
            Err(status::Custom(Status::InternalServerError, response.msg))
        }
    }
}
