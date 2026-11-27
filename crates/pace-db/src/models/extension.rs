use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ExtensionPairingCode {
    pub id: Uuid,
    pub user_id: Uuid,
    pub code_hash: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ExtensionToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub device_name: String,
    pub token_hash: String,
    pub last_project_id: Option<Uuid>,
    pub last_used_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
