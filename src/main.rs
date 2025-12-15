mod databases;
mod entities;
mod routes;
mod services;

use crate::databases::Tasks;
use crate::entities::config::Config;
use crate::routes::{chat, file, task};
use rocket::fairing::AdHoc;
use rocket::{launch, routes};
use rocket_db_pools::Database;

#[launch]
fn rocket() -> _ {
    rocket::build()
        .attach(Tasks::init())
        .attach(AdHoc::config::<Config>())
        .mount("/chat", routes![chat::completion, chat::stream])
        .mount("/task", routes![task::create, task::query, task::result])
        .mount("/file", routes![file::upload, file::download])
}

#[cfg(test)]
mod tests {
    use crate::entities::message::Message;
    use crate::entities::task::{Status, Task};
    use crate::rocket;
    use crate::routes::{chat, task};
    use agentx::{Completion, Role};
    use rocket::http::Status as HttpStatus;
    use rocket::local::blocking::Client;
    use rocket::uri;
    use serde_json::Value;

    #[test]
    fn test_chat() {
        let client = Client::tracked(rocket()).unwrap();
        let response = client
            .post(uri!("/chat", chat::completion(model = _)))
            .json(&Message {
                role: Some(Role::User),
                text: Some("你是谁".to_string()),
                images: None,
                videos: None,
                context: None,
            })
            .dispatch();
        assert_eq!(response.status(), HttpStatus::Ok);
        let mut json: Value = response.into_json().unwrap();
        println!("{:?}", json);
        assert!(json["success"].as_bool().unwrap());
        let completion: Completion = serde_json::from_value(json["data"].take()).unwrap();
        println!("{:?}", completion);
    }

    #[test]
    fn test_task() {
        let client = Client::tracked(rocket()).unwrap();
        let response = client
            .post(uri!(
                "/task",
                task::create(model = Some("qwen-vl-plus-2025-08-15"))
            ))
            .json(&Message {
                role: None,
                text: Some("这是什么".to_string()),
                images: Some(vec!["https://www.baidu.com/img/bd_logo.png".to_string()]),
                videos: None,
                context: None,
            })
            .dispatch();
        assert_eq!(response.status(), HttpStatus::Ok);
        let mut json: Value = response.into_json().unwrap();
        assert!(json["success"].as_bool().unwrap());
        let task: Task = serde_json::from_value(json["data"].take()).unwrap();
        let response = client
            .get(uri!("/task", task::result(id = task.id, timeout = _)))
            .dispatch();
        assert_eq!(response.status(), HttpStatus::Ok);
        let mut json: Value = response.into_json().unwrap();
        assert!(json["success"].as_bool().unwrap());
        let task: Task = serde_json::from_value(json["data"].take()).unwrap();
        println!("{:?}", task);
        assert_eq!(task.status, Status::Finished);
    }
}
