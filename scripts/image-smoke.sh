#!/usr/bin/env bash
# Run the image as the default USER (no --user). Probe protocol + metrics.
set -euo pipefail

IMAGE=${1:?usage: image-smoke.sh IMAGE}
NAME=fac-smoke
HTTP=18080
METRICS=19090
# Anvil account 0. Same constant as crates/facilitator/tests/compose_supported.rs.
# Construction-only. Do not broadcast.
export FACILITATOR_EVM_KEY=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
export FACILITATOR_API_TOKEN=image-smoke-token
ANVIL_ADDR=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266

cleanup() {
  docker rm -f "$NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT
cleanup

user=$(docker inspect -f '{{.Config.User}}' "$IMAGE")
case "$user" in
  65532|65532:65532) ;;
  *)
    echo "image-smoke: Config.User=$user want 65532" >&2
    exit 1
    ;;
esac

docker run -d --name "$NAME" --platform linux/amd64 \
  -e FACILITATOR_EVM_KEY \
  -e FACILITATOR_API_TOKEN \
  -e FACILITATOR_HTTP_LISTEN=0.0.0.0:8080 \
  -e FACILITATOR_HTTP_METRICS_LISTEN=0.0.0.0:9090 \
  -p 127.0.0.1:${HTTP}:8080 -p 127.0.0.1:${METRICS}:9090 \
  "$IMAGE" >/dev/null
run_user=$(docker inspect -f '{{.Config.User}}' "$NAME")
case "$run_user" in
  65532|65532:65532) ;;
  *)
    echo "image-smoke: container User=$run_user want 65532" >&2
    exit 1
    ;;
esac

up=0
for _ in $(seq 1 60); do
  if curl -fsS -o /dev/null --max-time 2 "http://127.0.0.1:${HTTP}/healthz"; then
    up=1
    break
  fi
  st=$(docker inspect -f '{{.State.Status}} {{.State.ExitCode}}' "$NAME")
  if [[ "$st" == exited* ]]; then
    echo "image-smoke: container exited: $st" >&2
    docker logs "$NAME" >&2 || true
    exit 1
  fi
  sleep 1
done
if [[ "$up" -ne 1 ]]; then
  echo "image-smoke: /healthz timeout" >&2
  docker logs "$NAME" >&2 || true
  exit 1
fi

python3 - "$HTTP" "$METRICS" "$ANVIL_ADDR" << 'PY'
import json, subprocess, sys

http, metrics, anvil = sys.argv[1], sys.argv[2], sys.argv[3]
AUTH = {"authorization": "Bearer image-smoke-token"}


def curl(method, url, data=None, timeout=20, headers=None):
    cmd = [
        "curl", "-sS", "-o", "/tmp/fac-smoke-body", "-w", "%{http_code}",
        "--max-time", str(timeout), "-X", method, url,
    ]
    if headers:
        for key, value in headers.items():
            cmd += ["-H", f"{key}: {value}"]
    if data is not None:
        cmd += ["-H", "content-type: application/json", "--data", data]
    p = subprocess.run(cmd, capture_output=True, text=True)
    code = int(p.stdout.strip() or "0")
    body = open("/tmp/fac-smoke-body", "rb").read()
    if p.returncode != 0 and code == 0:
        raise SystemExit(f"curl failed {url}: {p.stderr}")
    return code, body


def must(ok, msg):
    if not ok:
        raise SystemExit(f"image-smoke: {msg}")


def jload(raw):
    return json.loads(raw)


st, raw = curl("GET", f"http://127.0.0.1:{http}/healthz")
j = jload(raw)
must(st == 200 and j.get("status") == "ok", f"/healthz {st} {raw!r}")

st, raw = curl("GET", f"http://127.0.0.1:{http}/readyz")
j = jload(raw)
must(st == 200 and j.get("status") == "ok", f"/readyz {st} {raw!r}")

st, raw = curl("GET", f"http://127.0.0.1:{http}/supported")
j = jload(raw)
must(st == 401 and j.get("error") == "unauthorized", f"/supported unauth {st} {raw!r}")

st, raw = curl("GET", f"http://127.0.0.1:{http}/supported", headers=AUTH)
j = jload(raw)
must(st == 200, f"/supported {st} {raw!r}")
kinds = j.get("kinds") or []
pairs = {(k.get("network"), k.get("scheme")) for k in kinds}
want = {
    ("eip155:84532", "exact"),
    ("eip155:84532", "upto"),
    ("eip155:8453", "exact"),
    ("eip155:8453", "upto"),
}
must(pairs == want, f"/supported kinds {pairs}")
must(all(k.get("x402Version") == 2 for k in kinds), "/supported x402Version")
signers = json.dumps(j.get("signers") or {}).lower()
must(anvil.lower() in json.dumps(j).lower(), f"/supported missing {anvil}")
must("eip155:" in signers or anvil.lower() in json.dumps(j).lower(), "/supported signers")

for path in ("/", "/health", "/metrics"):
    st, raw = curl("GET", f"http://127.0.0.1:{http}{path}")
    must(st == 404, f"GET {path} {st}")

st, raw = curl("POST", f"http://127.0.0.1:{http}/verify", data="{", headers=AUTH)
j = jload(raw)
must(st == 400 and j.get("error") == "invalid request body", f"unparseable {st} {raw!r}")

st, raw = curl(
    "POST",
    f"http://127.0.0.1:{http}/verify",
    data='{"x402Version":1,"paymentPayload":{},"paymentRequirements":{}}',
    headers=AUTH,
)
j = jload(raw)
must(
    st == 200 and j.get("isValid") is False and j.get("invalidReason") == "invalid_x402_version",
    f"v1 verify {st} {raw!r}",
)

st, raw = curl(
    "POST",
    f"http://127.0.0.1:{http}/verify",
    data='{"x402Version":2,"paymentPayload":{},"paymentRequirements":{"network":"eip155:84532"}}',
    headers=AUTH,
)
j = jload(raw)
must(
    st == 200 and j.get("isValid") is False and j.get("invalidReason") == "invalid_payload",
    f"v2 empty {st} {raw!r}",
)

st, raw = curl(
    "POST",
    f"http://127.0.0.1:{http}/settle",
    data='{"x402Version":1,"paymentPayload":{},"paymentRequirements":{}}',
    headers=AUTH,
)
j = jload(raw)
must(
    st == 200
    and j.get("success") is False
    and j.get("errorReason") == "invalid_x402_version",
    f"v1 settle {st} {raw!r}",
)

st, raw = curl("GET", f"http://127.0.0.1:{metrics}/metrics")
must(st == 200, f"metrics {st}")
text = raw.decode("utf-8", "replace")
must("r402_facilitator_verify_total" in text, f"metrics missing verify_total {text[:200]!r}")

print("image-smoke: ok")
PY
