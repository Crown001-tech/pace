use crate::models::{AuditEntry, CreateAuditEntry};
use crate::pool::DbError;
use sqlx::PgPool;

pub async fn record_audit_entry(
    pool: &PgPool,
    entry: CreateAuditEntry,
) -> Result<AuditEntry, DbError> {
    let details = entry.details.unwrap_or_else(|| serde_json::json!({}));
    let audit = sqlx::query_as!(
        AuditEntry,
        r#"
        INSERT INTO audit_log (actor_id, action, target_type, target_id, details)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, actor_id, action, target_type, target_id, details, created_at
        "#,
        entry.actor_id,
        entry.action,
        entry.target_type,
        entry.target_id,
        details
    )
    .fetch_one(pool)
    .await?;

    Ok(audit)
}
