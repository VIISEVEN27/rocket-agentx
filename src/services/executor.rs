use crate::databases::Tasks;
use crate::entities::config::{ExecutorConfig, ServiceConfig};
use crate::entities::datetime::DateTime;
use crate::entities::task::{Status, Task};
use crate::services::models::{Qwen3, Qwen3VL};
use crate::services::{Inject, Service};
use agentx::Completion;
use anyhow::anyhow;
use async_compression::tokio::write::{ZstdDecoder, ZstdEncoder};
use futures::StreamExt;
use rocket_db_pools::deadpool_redis::redis::AsyncCommands;
use rocket_db_pools::Connection;
use state::InitCell;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tokio::time;

#[derive(Clone)]
pub struct Executor {
    config: Arc<ExecutorConfig>,
}

impl Inject for Executor {
    fn new(config: &ServiceConfig) -> Self {
        Self {
            config: Arc::new(config.executor.clone()),
        }
    }
}

static SEMAPHORE: InitCell<Arc<Semaphore>> = InitCell::new();
static PENDING_QUEUE: &str = "PENDING_QUEUE";

impl Executor {
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
                                eprintln!("Failed to execute: {:?}", err);
                            }
                        }
                        Ok(None) => break,
                        Err(err) => eprintln!("Failed to consume: {:?}", err),
                    }
                }
                drop(permit);
            });
        }
        Ok(())
    }

    async fn consume(&self, conn: &mut Connection<Tasks>) -> anyhow::Result<Option<Task>> {
        if let Some((_, task_id)) = conn
            .brpop::<&str, Option<((), String)>>(PENDING_QUEUE, self.config.timeout as f64)
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
        let result = if task.prompt.is_media() {
            let model = Service::<Qwen3VL>::inject();
            model.stream(&task.prompt).await
        } else {
            let model = Service::<Qwen3>::inject();
            model.stream(&task.prompt).await
        };
        match result {
            Ok(mut stream) => {
                task.status = Status::Finished;
                task.finish_time = Some(DateTime::local());
                let mut encoder = ZstdEncoder::new(Vec::new());
                let json = serde_json::to_string(&task)?;
                let (partial, _) = json.rsplit_once("}").unwrap();
                encoder.write(partial.as_bytes()).await?;
                encoder
                    .write(",\"completion\":{\"reasoning_content\":\"".as_bytes())
                    .await?;
                let mut reasoning = true;
                let mut usage_encoded = None;
                while let Some(chunk) = stream.next().await {
                    let Completion {
                        reasoning_content,
                        content,
                        usage,
                    } = chunk;
                    if let Some(reasoning_content) = reasoning_content {
                        encoder.write(reasoning_content.as_bytes()).await?;
                    }
                    if let Some(content) = content {
                        if reasoning {
                            encoder.write("\",\"content\":\"".as_bytes()).await?;
                            reasoning = false;
                        }
                        encoder.write(content.as_bytes()).await?;
                    }
                    if let Some(usage) = usage {
                        usage_encoded = Some(usage)
                    }
                }
                if let Some(usage) = usage_encoded {
                    encoder.write("\",\"usage\":".as_bytes()).await?;
                    encoder
                        .write(serde_json::to_string(&usage)?.as_bytes())
                        .await?;
                } else {
                    encoder.write("\",\"usage\":null".as_bytes()).await?;
                }
                encoder.write("}}".as_bytes()).await?;
                encoder.shutdown().await?;
                self.set_raw(conn, &task.id, encoder.into_inner()).await?;
            }
            Err(err) => {
                task.status = Status::Failed;
                task.err_msg = Some(err.to_string());
                self.set(conn, &task).await?;
            }
        }
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
                if timeout > 0 && now.elapsed().as_secs() >= timeout {
                    return Ok(task);
                }
                interval.tick().await;
            } else {
                return Err(anyhow!("Task '{task_id}' not existed"));
            }
        }
    }

    pub async fn get(
        &self,
        conn: &mut Connection<Tasks>,
        task_id: &str,
    ) -> anyhow::Result<Option<Task>> {
        if let Some(value) = conn.get::<&str, Option<Vec<u8>>>(task_id).await? {
            let json = self.decompress(&value).await?;
            let task = serde_json::from_str(&json)?;
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    async fn set(&self, conn: &mut Connection<Tasks>, task: &Task) -> anyhow::Result<()> {
        let value = self.compress(serde_json::to_string(task)?).await?;
        let _: () = conn.set_ex(&task.id, value, self.config.expiration).await?;
        Ok(())
    }

    async fn set_raw<K: AsRef<str>>(
        &self,
        conn: &mut Connection<Tasks>,
        key: K,
        value: Vec<u8>,
    ) -> anyhow::Result<()> {
        let _: () = conn
            .set_ex(key.as_ref(), value, self.config.expiration)
            .await?;
        Ok(())
    }

    async fn compress<T: AsRef<str>>(&self, data: T) -> anyhow::Result<Vec<u8>> {
        let mut encoder = ZstdEncoder::new(Vec::new());
        encoder.write(data.as_ref().as_bytes()).await?;
        encoder.shutdown().await?;
        Ok(encoder.into_inner())
    }

    async fn decompress(&self, data: &[u8]) -> anyhow::Result<String> {
        let mut decoder = ZstdDecoder::new(Vec::new());
        decoder.write_all(data).await?;
        decoder.shutdown().await?;
        let decompressed = String::from_utf8_lossy(&decoder.into_inner()).to_string();
        Ok(decompressed.replace("\n", "\\n"))
    }
}
