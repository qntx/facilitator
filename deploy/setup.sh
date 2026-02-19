#!/usr/bin/env bash
# ==========================================================================
# x402 Facilitator — One-Click Deployment Script
#
# Idempotent & resumable: safe to re-run after failures.
#
# Usage:
#   sudo bash setup.sh            # Full setup
#   sudo bash setup.sh --check    # Pre-flight checks only
#   sudo bash setup.sh --force    # Ignore saved state, redo all steps
#
# Prerequisites:
#   - Ubuntu 22.04+ / Debian 12+ (root or sudo)
#   - Domain DNS A record pointing to this server
# ==========================================================================

set -euo pipefail

# ── Colors & helpers ─────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[ OK ]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERR ]${NC}  $*"; }
die()   { err "$@"; exit 1; }
bold()  { echo -e "${BOLD}$*${NC}"; }

DEPLOY_DIR="/opt/facilitator"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STATE_FILE="${DEPLOY_DIR}/.setup-state"
REQUIRED_FILES=(docker-compose.yml Caddyfile config.example.toml fctl)
TOTAL_STEPS=6

# Parse flags
FLAG_CHECK=false
FLAG_FORCE=false
for arg in "$@"; do
    case "${arg}" in
        --check) FLAG_CHECK=true ;;
        --force) FLAG_FORCE=true ;;
    esac
done

# ── State management (resume support) ────────────────────────────────────
step_done() {
    [[ "${FLAG_FORCE}" == "true" ]] && return 1
    [[ -f "${STATE_FILE}" ]] && grep -qx "DONE:$1" "${STATE_FILE}" 2>/dev/null
}

mark_done() {
    mkdir -p "${DEPLOY_DIR}"
    echo "DONE:$1" >> "${STATE_FILE}"
}

step() {
    local n=$1; shift
    if step_done "${n}"; then
        ok "Step ${n}/${TOTAL_STEPS}: $* (already done, skipping)"
        return 1
    fi
    info "Step ${n}/${TOTAL_STEPS}: $*"
    return 0
}

# ── Root check ───────────────────────────────────────────────────────────
[[ $EUID -eq 0 ]] || die "Please run as root:  sudo bash setup.sh"

echo ""
echo -e "${CYAN}╔══════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║       x402 Facilitator — Deployment Setup        ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════════════╝${NC}"
echo ""

# ── Pre-flight checks ───────────────────────────────────────────────────
info "Running pre-flight checks..."
preflight_ok=true

# Check OS
if [[ -f /etc/os-release ]]; then
    . /etc/os-release
    ok "OS: ${PRETTY_NAME:-${ID} ${VERSION_ID}}"
else
    warn "Cannot detect OS"; preflight_ok=false
fi

# Check disk space (need > 2GB)
avail=$(df -BG / 2>/dev/null | awk 'NR==2{print $4}' | tr -d 'G')
if [[ -n "${avail}" ]] && [[ "${avail}" -ge 2 ]]; then
    ok "Disk: ${avail}GB available"
else
    err "Disk: ${avail:-unknown}GB available (need >= 2GB)"; preflight_ok=false
fi

# Check memory (need > 512MB)
mem=$(free -m 2>/dev/null | awk '/^Mem:/{print $2}')
if [[ -n "${mem}" ]] && [[ "${mem}" -ge 512 ]]; then
    ok "Memory: ${mem}MB total"
else
    warn "Memory: ${mem:-unknown}MB total (recommend >= 512MB)"
fi

# Check port availability
for port in 80 443; do
    if ss -tlnp 2>/dev/null | grep -q ":${port} "; then
        warn "Port ${port} already in use (may conflict with Caddy)"
    else
        ok "Port ${port}: available"
    fi
done

# Check source files
for f in "${REQUIRED_FILES[@]}"; do
    if [[ -f "${SCRIPT_DIR}/${f}" ]]; then
        ok "Source: ${f}"
    elif [[ -f "${DEPLOY_DIR}/${f}" ]]; then
        ok "Source: ${f} (in deploy dir)"
    else
        err "Missing: ${f}"; preflight_ok=false
    fi
done

echo ""
if [[ "${FLAG_CHECK}" == "true" ]]; then
    [[ "${preflight_ok}" == "true" ]] && ok "All pre-flight checks passed." || err "Some checks failed."
    exit 0
fi

[[ "${preflight_ok}" == "true" ]] || die "Pre-flight checks failed. Fix the issues above and re-run."

# ── Step 1: System update ────────────────────────────────────────────────
if step 1 "Updating system packages..."; then
    apt-get update -qq && apt-get upgrade -y -qq
    mark_done 1
    ok "System updated"
fi

# ── Step 2: Install Docker + Compose ─────────────────────────────────────
if step 2 "Installing Docker..."; then
    if command -v docker &>/dev/null; then
        ok "Docker already installed: $(docker --version)"
    else
        curl -fsSL https://get.docker.com | sh
        systemctl enable --now docker
        ok "Docker installed: $(docker --version)"
    fi

    docker compose version &>/dev/null \
        || die "Docker Compose plugin not found. Install: apt-get install docker-compose-plugin"
    ok "Docker Compose: $(docker compose version --short)"
    mark_done 2
fi

# ── Step 3: Deploy files ─────────────────────────────────────────────────
if step 3 "Copying files to ${DEPLOY_DIR}..."; then
    mkdir -p "${DEPLOY_DIR}"

    for f in "${REQUIRED_FILES[@]}"; do
        if [[ -f "${SCRIPT_DIR}/${f}" ]]; then
            cp "${SCRIPT_DIR}/${f}" "${DEPLOY_DIR}/${f}"
        elif [[ ! -f "${DEPLOY_DIR}/${f}" ]]; then
            die "Missing ${f} — cannot continue."
        fi
    done

    # Copy config.toml if user pre-configured it in the source directory
    if [[ -f "${SCRIPT_DIR}/config.toml" && ! -f "${DEPLOY_DIR}/config.toml" ]]; then
        cp "${SCRIPT_DIR}/config.toml" "${DEPLOY_DIR}/config.toml"
        chmod 644 "${DEPLOY_DIR}/config.toml"
        ok "config.toml copied from source directory"
    fi

    # Make fctl executable and install to PATH
    chmod +x "${DEPLOY_DIR}/fctl"
    ln -sf "${DEPLOY_DIR}/fctl" /usr/local/bin/fctl
    ok "fctl installed to /usr/local/bin/fctl"

    mark_done 3
    ok "Deploy files ready"
fi

# ── Step 4: Generate config.toml from example ───────────────────────────
if step 4 "Checking facilitator config..."; then
    if [[ ! -f "${DEPLOY_DIR}/config.toml" ]]; then
        cp "${DEPLOY_DIR}/config.example.toml" "${DEPLOY_DIR}/config.toml"
        chmod 644 "${DEPLOY_DIR}/config.toml"
        warn "config.toml created — please edit it with your signer private keys:"
        warn "  nano ${DEPLOY_DIR}/config.toml"
        echo ""
        if [[ -t 0 ]]; then
            read -rp "Press Enter after editing (or Ctrl+C to abort, re-run setup.sh to resume)... "
        else
            warn "Non-interactive mode — add signer keys before first use!"
        fi
    else
        ok "config.toml already exists"
    fi
    mark_done 4
fi

# ── Step 5: Pull images ─────────────────────────────────────────────────
if step 5 "Pulling Docker images..."; then
    cd "${DEPLOY_DIR}"
    docker compose pull
    mark_done 5
    ok "Images pulled"
fi

# ── Step 6: Start services ──────────────────────────────────────────────
if step 6 "Starting services..."; then
    cd "${DEPLOY_DIR}"

    # Auto-clean stale x402-* containers that may block fresh deployment
    stale=$(docker ps -a --filter "name=x402-" --format "{{.Names}}" 2>/dev/null) || true
    if [[ -n "${stale}" ]]; then
        warn "Found stale containers — cleaning up first..."
        echo "${stale}" | while IFS= read -r c; do
            docker rm -f "${c}" 2>/dev/null && ok "Removed stale: ${c}" || true
        done
    fi

    docker compose up -d --remove-orphans
    mark_done 6
    ok "Services started"
fi

# ── Health check ─────────────────────────────────────────────────────────
echo ""
info "Waiting for health check..."
MAX_RETRIES=15
RETRY_INTERVAL=4
healthy=false

for i in $(seq 1 "${MAX_RETRIES}"); do
    if curl -sf http://localhost:8080/health >/dev/null 2>&1; then
        ok "Facilitator is healthy!"
        healthy=true
        break
    fi
    [[ $i -lt ${MAX_RETRIES} ]] && sleep "${RETRY_INTERVAL}"
done

if [[ "${healthy}" == "false" ]]; then
    warn "Health check timed out after $((MAX_RETRIES * RETRY_INTERVAL))s"
    warn "This is normal on first boot. Check: fctl logs"
fi

# Save config checksums for smart reload
cd "${DEPLOY_DIR}"
for f in config.toml Caddyfile docker-compose.yml; do
    [[ -f "${f}" ]] && md5sum "${f}"
done > "${DEPLOY_DIR}/.config-checksums" 2>/dev/null || true

# Clean up state file on success
rm -f "${STATE_FILE}"

# ── Done ─────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}╔══════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║            Deployment complete!                  ║${NC}"
echo -e "${GREEN}╚══════════════════════════════════════════════════╝${NC}"
echo ""
bold "  Quick Reference:"
echo ""
echo "    fctl status       Dashboard"
echo "    fctl logs         View facilitator logs"
echo "    fctl doctor       Run diagnostics"
echo "    fctl edit config  Edit config.toml (auto-backup + reload)"
echo "    fctl deploy       Redeploy after config changes"
echo "    fctl update       Pull latest images"
echo "    fctl help         All commands"
echo ""
info "Next steps:"
echo "  1. Edit config:    fctl edit config   (signer keys, chains, RPC)"
echo "  2. Edit domain:    fctl edit caddy    (replace facilitator.qntx.fun)"
echo "  3. Verify:         curl https://YOUR_DOMAIN/health"
echo ""