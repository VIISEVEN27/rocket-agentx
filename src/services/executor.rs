use crate::databases::Tasks;
use crate::entities::config::{ExecutorConfig, ServiceConfig};
use crate::entities::task::{Status, Task};
use crate::services::{Inject, Service};
use agentx::StreamingChatModel;
use anyhow::anyhow;
use rocket_db_pools::deadpool_redis::redis::AsyncCommands;
use rocket_db_pools::Connection;
use state::InitCell;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time;

pub struct Executor<T: StreamingChatModel + Inject> {
    model: Arc<Service<T>>,
    config: Arc<ExecutorConfig>,
}

impl<T: StreamingChatModel + Inject> Clone for Executor<T> {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            config: self.config.clone(),
        }
    }
}

impl<T: StreamingChatModel + Inject> Inject for Executor<T> {
    fn new(config: &ServiceConfig) -> Self {
        Self {
            model: Arc::new(Service::new(config)),
            config: Arc::new(config.executor.clone()),
        }
    }
}

static SEMAPHORE: InitCell<Arc<Semaphore>> = InitCell::new();
static PENDING_QUEUE: &str = "PENDING_QUEUE";

impl<T: StreamingChatModel + Inject> Executor<T> {
    pub async fn submit(&self, mut conn: Connection<Tasks>, task: &Task) -> anyhow::Result<()> {
        self.set(&mut conn, task).await?;
        let _: () = conn.lpush(PENDING_QUEUE, &task.id).await?;
        let semaphore = SEMAPHORE.get_or_init(|| Arc::new(Semaphore::new(self.config.num_workers)));
        if let Ok(permit) = semaphore.try_acquire() {
            let executor = self.clone();
            tokio::spawn(async move {
                loop {
                    match executor.consume(&mut conn).await {
                        Ok(Some(task)) => {
                            if let Err(err) = executor.execute(&mut conn, task).await {
                                eprintln!("Failed to execute: {err}");
                            }
                        }
                        Ok(None) => break,
                        Err(err) => eprintln!("Failed to consume: {err}"),
                    }
                }
                drop(permit);
            });
        }
        Ok(())
    }

    async fn consume(&self, conn: &mut Connection<Tasks>) -> anyhow::Result<Option<Task>> {
        if let Some((_, task_id)) = conn
            .brpop::<&str, Option<((), String)>>(PENDING_QUEUE, self.config.lifetime as f64)
            .await?
        {
            if let Some(task) = self.get(conn, &task_id).await? {
                Ok(Some(task))
            } else {
                Err(anyhow!("Task '{task_id}' not existed"))
            }
        } else {
            Ok(None)
        }
    }

    async fn execute(&self, conn: &mut Connection<Tasks>, mut task: Task) -> anyhow::Result<()> {
        task.status = Status::Running;
        self.set(conn, &task).await?;
        task.execute(&self.model).await;
        self.set(conn, &task).await?;
        Ok(())
    }

    pub async fn result(
        &self,
        mut conn: Connection<Tasks>,
        task_id: &String,
        timeout: u64,
    ) -> anyhow::Result<Task> {
        let now = Instant::now();
        let mut interval = time::interval(Duration::from_secs(1));
        interval.tick().await;
        loop {
            if let Some(task) = self.get(&mut conn, task_id).await? {
                if task.status == Status::Finished || task.status == Status::Failed {
                    return Ok(task);
                }
            } else {
                return Err(anyhow!("Task '{task_id}' not existed"));
            }
            if timeout > 0 && now.elapsed().as_secs() >= timeout {
                return Err(anyhow!("Timeout '{timeout}s' exceeded"));
            }
            interval.tick().await;
        }
    }

    pub async fn get(
        &self,
        conn: &mut Connection<Tasks>,
        task_id: &String,
    ) -> anyhow::Result<Option<Task>> {
        if let Some(json) = conn.get::<&String, Option<String>>(task_id).await? {
            Ok(Some(serde_json::from_str(&json)?))
        } else {
            Ok(None)
        }
    }

    async fn set(&self, conn: &mut Connection<Tasks>, task: &Task) -> anyhow::Result<()> {
        let _: () = conn
            .set_ex(
                &task.id,
                serde_json::to_string(task)?,
                self.config.expiration,
            )
            .await?;
        Ok(())
    }
}
