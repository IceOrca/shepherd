use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JobEnvelope {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub kind: String,
    pub payload: serde_json::Value,
}

#[async_trait]
pub trait JobQueue: Send + Sync {
    async fn enqueue(&self, job: JobEnvelope) -> Result<(), String>;
}
