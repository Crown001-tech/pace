#!/usr/bin/env bash
set -euo pipefail

# scripts/create-admin.sh — Creates the first administrator user in PACE
# Usage: ./scripts/create-admin.sh [email] [password] [name]

EMAIL="${1:-admin@pace.local}"
PASSWORD="${2:-AdminPassword123!}"
NAME="${3:-System Administrator}"

echo "Creating PACE administrator user..."
cargo run -p pace-api --bin create-admin -- "$EMAIL" "$PASSWORD" "$NAME"
