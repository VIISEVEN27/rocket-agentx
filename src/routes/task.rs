use crate::databases::Tasks;
use crate::entities::message::Message;
use crate::entities::response::Response;
use crate::entities::task::Task;
use crate::services::executor::Executor;
use crate::services::model::Model;
use crate::services::Service;
use rocket::serde::json::Json;
use rocket::{get, post};
use rocket_db_pools::Connection;

#[post("/create?<model>", data = "<message>")]
pub async fn create(
    model: Option<String>,
    message: Json<Message>,
    executor: &Service<Executor<Model>>,
    conn: Connection<Tasks>,
) -> Json<Response<Task>> {
    Response::invoke(async {
        let task = Task::create(model, message.into_inner());
        executor.submit(conn, &task).await?;
        Ok(task)
    })
    .await
    .into()
}

#[get("/query?<id>")]
pub async fn query(
    id: String,
    executor: &Service<Executor<Model>>,
    mut conn: Connection<Tasks>,
) -> Json<Response<Option<Task>>> {
    Response::invoke(async { executor.get(&mut conn, &id).await })
        .await
        .into()
}

#[get("/result?<id>&<timeout>")]
pub async fn result(
    id: String,
    timeout: Option<u64>,
    executor: &Service<Executor<Model>>,
    conn: Connection<Tasks>,
) -> Json<Response<Task>> {
    Response::invoke(async { executor.result(conn, &id, timeout.unwrap_or(0)).await })
        .await
        .into()
}
