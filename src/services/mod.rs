pub mod executor;
pub mod models;
pub mod oss;

use std::ops::Deref;

use anyhow::anyhow;
use ref_cast::RefCast;
use rocket::{
    http::Status,
    request::{FromRequest, Outcome},
    Request, State,
};
use state::TypeMap;

use crate::entities::config::{Config, ServiceConfig};

pub trait Inject: Send + Sync {
    fn new(config: &ServiceConfig) -> Self;
}

#[derive(RefCast)]
#[repr(transparent)]
pub struct Service<T: Inject>(T);

static SERVICES: TypeMap![Send + Sync] = <TypeMap![Send + Sync]>::new();

impl<T: Inject> Service<T> {
    fn new(config: &ServiceConfig) -> Self {
        Self(T::new(config))
    }

    fn register(config: &ServiceConfig) -> &'static Self {
        SERVICES.set(T::new(config));
        Self::ref_cast(SERVICES.get())
    }

    fn inject(config: &ServiceConfig) -> &'static Self {
        SERVICES.try_get().unwrap_or_else(|| Self::register(config))
    }
}

impl<T: Inject> Deref for Service<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[rocket::async_trait]
impl<'r, T: Inject> FromRequest<'r> for Service<T> {
    type Error = anyhow::Error;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let config = match request.guard::<&State<Config>>().await {
            Outcome::Success(config) => config,
            _ => {
                return Outcome::Error((
                    Status::InternalServerError,
                    anyhow!("State 'Config' not existed"),
                ));
            }
        };
        Outcome::Success(Self::new(&config.services))
    }
}

#[rocket::async_trait]
impl<'r, T: Inject> FromRequest<'r> for &'static Service<T> {
    type Error = anyhow::Error;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let config = match request.guard::<&State<Config>>().await {
            Outcome::Success(config) => config,
            _ => {
                return Outcome::Error((
                    Status::InternalServerError,
                    anyhow!("State 'Config' not existed"),
                ));
            }
        };
        Outcome::Success(Service::inject(&config.services))
    }
}
