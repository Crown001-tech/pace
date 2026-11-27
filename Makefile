.PHONY: help up down logs reset test

help:
	@echo "PACE Developer Commands:"
	@echo "  make up     - Start PostgreSQL and MinIO containers"
	@echo "  make down   - Stop containers (preserving data volumes)"
	@echo "  make logs   - Tail logs from all running containers"
	@echo "  make reset  - Stop containers and DELETE all local volumes (fresh start)"
	@echo "  make test   - Run cargo test across the workspace"

up:
	docker compose -f infra/docker/compose.yml up -d

down:
	docker compose -f infra/docker/compose.yml down

logs:
	docker compose -f infra/docker/compose.yml logs -f

reset:
	docker compose -f infra/docker/compose.yml down -v

test:
	cargo test
