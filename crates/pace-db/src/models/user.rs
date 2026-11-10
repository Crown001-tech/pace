use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    #[default]
    Creator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub password_hash: String,
    pub role: String,
    pub must_change_password: bool,
    pub is_active: bool,
    pub storage_quota_bytes: i64,
    pub storage_used_bytes: i64,
    pub daily_1080p_limit: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUser {
    pub email: String,
    pub name: String,
    pub password_hash: String,
    pub role: String,
    pub must_change_password: bool,
    pub storage_quota_bytes: Option<i64>,
}
