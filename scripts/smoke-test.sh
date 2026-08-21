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
printf '%s\n' 'admin:0123456789abcdef0123456789abcdef' >"$work_dir/admin-auth.txt"
chmod 0600 "$work_dir/config.json" "$work_dir/admin-auth.txt"

"$manager" \
  --config "$work_dir/config.json" \
  --admin-auth-file "$work_dir/admin-auth.txt" \
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

unauthorized_status="$(curl -sS -o /dev/null -w '%{http_code}' "http://127.0.0.1:$manager_port/api/status")"
[[ "$unauthorized_status" == "401" ]]

status_body="$(curl -fsS -u admin:0123456789abcdef0123456789abcdef "http://127.0.0.1:$manager_port/api/status")"
grep -Fq '"running":true' <<<"$status_body"
grep -Fq '"password_set":false' <<<"$status_body"

updated_body="$(curl -fsS \
  -u admin:0123456789abcdef0123456789abcdef \
  -X PUT \
  -H 'Content-Type: application/json' \
  --data "$(sed 's/\"username\": \"\"/\"username\": \"smoke-user\"/; s/\"password\": \"\"/\"password\": \"miniserve-smoke-secret\"/' "$work_dir/config.json")" \
  "http://127.0.0.1:$manager_port/api/config")"
grep -Fq '"password_set":true' <<<"$updated_body"
if grep -Fq 'miniserve-smoke-secret' <<<"$updated_body"; then
  echo "The status API exposed the stored miniserve password" >&2
  exit 1
fi

curl -fsS -u smoke-user:miniserve-smoke-secret "http://127.0.0.1:$service_port/" >/dev/null
echo "Authenticated manager and miniserve smoke test passed."
