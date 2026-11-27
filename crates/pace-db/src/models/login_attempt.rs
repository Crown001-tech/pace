use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct LoginAttempt {
    pub id: i64,
    pub email: String,
    pub attempted_at: DateTime<Utc>,
    pub success: bool,
}
