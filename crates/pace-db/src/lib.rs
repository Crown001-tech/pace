//! pace-db: PostgreSQL schema, models, migrations, and queries for PACE.
//!
//! Enforces strict data ownership isolation: every query is filtered by owner.

pub mod models;
pub mod pool;
pub mod repo;

pub use models::*;
pub use pool::*;
pub use repo::*;
pub use sqlx::{self, PgPool as DbPool};
