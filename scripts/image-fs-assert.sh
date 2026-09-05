#!/usr/bin/env bash
# Assert /etc/facilitator is traversable by USER 65532 without starting serve.
set -euo pipefail

IMAGE=${1:?usage: image-fs-assert.sh IMAGE}
HERE=$(cd "$(dirname "$0")" && pwd)
cid=""
cleanup() {
  if [[ -n "$cid" ]]; then
    docker rm -f "$cid" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

cid=$(docker create --platform linux/amd64 "$IMAGE")
docker export "$cid" | python3 "$HERE/image-fs-assert.py"
