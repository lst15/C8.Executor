pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;

use std::sync::Arc;

use adapters::{
    inbound::http::build_router,
    outbound::{in_memory::InMemoryPorts, kafka::KafkaEventPublisher, postgres::PostgresPorts},
};
use application::service::ExecutorService;
use domain::error::AppError;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub kafka_brokers: String,
    pub kafka_topic: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, AppError> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| AppError::Config("DATABASE_URL is required".to_string()))?;
        let kafka_brokers = std::env::var("KAFKA_BROKERS")
            .map_err(|_| AppError::Config("KAFKA_BROKERS is required".to_string()))?;
        let kafka_topic = std::env::var("KAFKA_TOPIC_C8_EVENTS")
            .unwrap_or_else(|_| "c8.executor.events".to_string());

        Ok(Self {
            database_url,
            kafka_brokers,
            kafka_topic,
        })
    }
}

pub async fn build_app_from_config(config: AppConfig) -> Result<axum::Router, AppError> {
    let postgres = Arc::new(PostgresPorts::connect(&config.database_url).await?);
    postgres.ensure_schema().await?;

    let kafka = Arc::new(KafkaEventPublisher::new(
        &config.kafka_brokers,
        &config.kafka_topic,
    )?);

    let service = Arc::new(ExecutorService::new(postgres.clone(), postgres, kafka));
    Ok(build_router(service))
}

pub fn build_app_in_memory() -> axum::Router {
    let ports = Arc::new(InMemoryPorts::new());
    let service = Arc::new(ExecutorService::new(ports.clone(), ports.clone(), ports));
    build_router(service)
}
