#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 MANAGER MINISERVE" >&2
  exit 2
fi

manager="$(readlink -f "$1")"
miniserve="$(readlink -f "$2")"
work_dir="$(mktemp -d /tmp/miniserve-qnap-smoke.XXXXXX)"
manager_port="$((19000 + $$ % 1000))"
service_port="$((20000 + $$ % 1000))"
manager_pid=""

cleanup() {
  if [[ -n "$manager_pid" ]] && kill -0 "$manager_pid" 2>/dev/null; then
    kill "$manager_pid" 2>/dev/null || true
    wait "$manager_pid" 2>/dev/null || true
  fi
  find "$work_dir" -depth -delete 2>/dev/null || true
}
trap cleanup EXIT

cat >"$work_dir/config.json" <<EOF
{
  "share_dir": "$work_dir",
  "listen_address": "127.0.0.1",
  "port": $service_port,
  "title": "QPKG smoke test",
  "route_prefix": "",
  "upload": false,
  "mkdir": false,
  "hidden": false,
  "follow_symlinks": false,
  "username": "",
  "password": "",
  "color_scheme": "squirrel",
  "sorting_method": "name",
  "sorting_order": "asc",
  "index_file": "",
  "pretty_urls": false
}
EOF
chmod 0600 "$work_dir/config.json"

"$manager" \
  --config "$work_dir/config.json" \
  --miniserve "$miniserve" \
  --listen "127.0.0.1:$manager_port" \
  >"$work_dir/manager.log" 2>&1 &
manager_pid=$!

for _ in {1..20}; do
  if curl -fsS "http://127.0.0.1:$manager_port/healthz" 2>/dev/null | grep -Fxq ok; then
    break
  fi
  kill -0 "$manager_pid"
  sleep 0.25
done
curl -fsS "http://127.0.0.1:$manager_port/healthz" | grep -Fxq ok

# Management authentication belongs to QTS; both proxy forwarding styles work.
for prefix in "" "/miniserve"; do
  page_body="$(curl -fsS "http://127.0.0.1:$manager_port$prefix/")"
  grep -Fq 'Miniserve 控制台' <<<"$page_body"
  status_body="$(curl -fsS "http://127.0.0.1:$manager_port$prefix/api/status")"
  grep -Fq '"running":true' <<<"$status_body"
  grep -Fq '"password_set":false' <<<"$status_body"
  curl -fsS "http://127.0.0.1:$manager_port$prefix/healthz" | grep -Fxq ok
  removed_status="$(curl -sS -o /dev/null -w '%{http_code}' -X PUT -H 'Content-Type: application/json' --data '{"new_password":"unused-password"}' "http://127.0.0.1:$manager_port$prefix/api/admin-password")"
  [[ "$removed_status" == "404" ]]
done
page_body="$(curl -fsS "http://127.0.0.1:$manager_port/miniserve")"
grep -Fq 'Miniserve 控制台' <<<"$page_body"
[[ ! -e "$work_dir/admin-auth.txt" ]]

# Non-local listening must fail before any configuration or child process starts.
for address in "0.0.0.0:$manager_port" "192.168.1.10:$manager_port" "[::]:$manager_port"; do
  if "$manager" --config "$work_dir/rejected.json" --miniserve "$miniserve" --listen "$address" >"$work_dir/rejected.log" 2>&1; then
    echo "Manager accepted external address: $address" >&2
    exit 1
  fi
  grep -Fq 'management server must listen on 127.0.0.1' "$work_dir/rejected.log"
  [[ ! -e "$work_dir/rejected.json" ]]
done

updated_body="$(curl -fsS \
  -X PUT \
  -H 'Content-Type: application/json' \
  --data "$(sed 's/\"username\": \"\"/\"username\": \"smoke-user\"/; s/\"password\": \"\"/\"password\": \"miniserve-smoke-secret\"/' "$work_dir/config.json")" \
  "http://127.0.0.1:$manager_port/miniserve/api/config")"
grep -Fq '"password_set":true' <<<"$updated_body"
if grep -Fq 'miniserve-smoke-secret' <<<"$updated_body"; then
  echo "The status API exposed the stored miniserve password" >&2
  exit 1
fi

curl -fsS -X POST "http://127.0.0.1:$manager_port/miniserve/api/restart" | grep -Fq '"running":true'
file_unauthorized_status="$(curl -sS -o /dev/null -w '%{http_code}' "http://127.0.0.1:$service_port/")"
[[ "$file_unauthorized_status" == "401" ]]
curl -fsS -u smoke-user:miniserve-smoke-secret "http://127.0.0.1:$service_port/" >/dev/null
echo "Loopback manager, proxy paths and authenticated miniserve smoke test passed."
