# x402 Facilitator — Makefile
#
# Thin wrapper around fctl. For the full CLI, run: fctl help
# Usage: make help

COMPOSE := docker compose
CTL     := $(if $(shell command -v fctl 2>/dev/null),fctl,bash fctl)

.DEFAULT_GOAL := help

# ── Lifecycle ────────────────────────────────────────────────────────

.PHONY: setup
setup: ## First-time deployment (install Docker, pull images, start)
	@sudo bash setup.sh

.PHONY: deploy
deploy: ## Pull latest images + recreate all containers + health check
	@$(CTL) deploy

.PHONY: reload
reload: ## Smart reload (auto-detect which config changed)
	@$(CTL) reload

.PHONY: update
update: ## Pull latest Docker images + rolling restart
	@$(CTL) update

.PHONY: up
up: ## Start all services
	@$(COMPOSE) up -d

.PHONY: down
down: ## Stop all services
	@$(COMPOSE) down

.PHONY: restart
restart: ## Restart all services
	@$(COMPOSE) restart

# ── Observability ────────────────────────────────────────────────────

.PHONY: status
status: ## Service dashboard (status, health, versions)
	@$(CTL) status

.PHONY: doctor
doctor: ## Run full diagnostics
	@$(CTL) doctor

.PHONY: logs
logs: ## Follow facilitator logs
	@$(CTL) logs facilitator

.PHONY: logs-caddy
logs-caddy: ## Follow Caddy logs
	@$(CTL) logs caddy

.PHONY: logs-watchtower
logs-watchtower: ## Follow Watchtower logs
	@$(CTL) logs watchtower

.PHONY: logs-all
logs-all: ## Follow all service logs
	@$(CTL) logs all

.PHONY: health
health: ## Quick health check
	@curl -sf http://localhost:8080/health > /dev/null \
		&& echo "✓ facilitator healthy" \
		|| echo "✗ facilitator unhealthy"

.PHONY: supported
supported: ## Show supported chains and schemes
	@curl -sf http://localhost:8080/supported | head -c 4096 && echo

# ── Configuration ────────────────────────────────────────────────────

.PHONY: edit-config
edit-config: ## Edit config.toml (auto-backup + reload)
	@$(CTL) edit config

.PHONY: edit-caddy
edit-caddy: ## Edit Caddyfile (auto-backup + reload)
	@$(CTL) edit caddy

.PHONY: backup
backup: ## Backup all config files
	@$(CTL) backup

# ── Maintenance ──────────────────────────────────────────────────────

.PHONY: prune
prune: ## Remove dangling Docker images to free disk
	@docker image prune -f

.PHONY: reset
reset: ## Stop all + remove volumes (destructive!)
	@$(CTL) reset

.PHONY: purge
purge: ## Force-remove ALL x402-* containers/volumes/networks
	@$(CTL) purge

# ── Help ─────────────────────────────────────────────────────────────

.PHONY: help
help: ## Show this help
	@echo ""
	@echo "  x402 Facilitator — Commands"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "  Full CLI: fctl help"
	@echo ""
