use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct Config {
    pub services: ServiceConfig,
}

#[derive(Deserialize, Clone)]
pub struct ServiceConfig {
    pub model: ModelConfig,
    pub executor: ExecutorConfig,
    pub oss: OSSConfig,
}

#[derive(Deserialize, Clone)]
pub struct ModelConfig {
    pub model: String,
    pub base_url: String,
    pub api_key: String,
}

#[derive(Deserialize, Clone)]
pub struct ExecutorConfig {
    pub num_workers: usize,
    pub lifetime: u64,
    pub expiration: u64,
}

#[derive(Deserialize, Clone)]
pub struct OSSConfig {
    pub prefix: String,
    pub bucket: String,
    pub endpoint: String,
    pub access_key_id: String,
    pub access_key_secret: String,
}
