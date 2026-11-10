# Milestone 2: Accounts and Projects API (Async Rust, Axum & SQLx)

1. Axum extractors enforce security boundaries declaratively at compile time, rejecting unauthorized requests before handler bodies execute.
2. Multi-tenancy isolation (AUTH-05) strictly requires 404 responses instead of 403 to prevent resource enumeration attacks.
3. Optimistic concurrency using If-Match ETags avoids database write locks while preventing lost updates across concurrent tabs.
