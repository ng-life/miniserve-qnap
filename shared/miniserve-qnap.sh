#!/bin/sh
CONF="/etc/config/qpkg.conf"
QPKG_NAME="miniserve-qnap"
QPKG_ROOT="$(/sbin/getcfg "$QPKG_NAME" Install_Path -f "$CONF")"
DATA_DIR="$QPKG_ROOT/var"
PID_FILE="$DATA_DIR/manager.pid"
LOG_FILE="$DATA_DIR/manager.log"
MANAGER="$QPKG_ROOT/bin/miniserve-qnap-manager"
MINISERVE="$QPKG_ROOT/bin/miniserve"
CONFIG_FILE="$DATA_DIR/config.json"
ADMIN_AUTH_FILE="$DATA_DIR/admin-auth.txt"
export QNAP_QPKG="$QPKG_NAME"

read_manager_pid() {
    [ -f "$PID_FILE" ] || return 1
    MANAGER_PID="$(cat "$PID_FILE" 2>/dev/null)"
    case "$MANAGER_PID" in
      ''|*[!0-9]*) return 1 ;;
    esac
}

manager_identity_matches() {
    read_manager_pid || return 1
    [ -e "/proc/$MANAGER_PID/exe" ] || return 1
    RUNNING_EXE="$(readlink "/proc/$MANAGER_PID/exe" 2>/dev/null)" || return 1
    [ "$RUNNING_EXE" = "$MANAGER" ]
}

is_running() {
    manager_identity_matches && kill -0 "$MANAGER_PID" 2>/dev/null
}

health_ready() {
    HEALTH_URL="http://127.0.0.1:8090/healthz"
    if command -v wget >/dev/null 2>&1; then
        [ "$(wget -q -T 2 -O - "$HEALTH_URL" 2>/dev/null)" = "ok" ]
    elif command -v curl >/dev/null 2>&1; then
        [ "$(curl -fsS --max-time 2 "$HEALTH_URL" 2>/dev/null)" = "ok" ]
    else
        return 1
    fi
}

start_service() {
    if is_running; then
        echo "$QPKG_NAME is already running."
        return 0
    fi
    mkdir -p "$DATA_DIR"
    chmod 700 "$DATA_DIR"
    if [ ! -x "$MANAGER" ] || [ ! -x "$MINISERVE" ]; then
        echo "Required executable is missing."
        return 1
    fi
    nohup "$MANAGER" \
        --config "$CONFIG_FILE" \
        --admin-auth-file "$ADMIN_AUTH_FILE" \
        --miniserve "$MINISERVE" \
        --listen "0.0.0.0:8090" \
        >>"$LOG_FILE" 2>&1 &
    MANAGER_PID=$!
    echo "$MANAGER_PID" > "$PID_FILE"
    chmod 600 "$PID_FILE"
    COUNT=0
    while [ "$COUNT" -lt 15 ]; do
        if ! manager_identity_matches || ! kill -0 "$MANAGER_PID" 2>/dev/null; then
            break
        fi
        if health_ready; then
            echo "$QPKG_NAME started with PID $MANAGER_PID."
            return 0
        fi
        sleep 1
        COUNT=$((COUNT + 1))
    done
    if manager_identity_matches; then
        kill "$MANAGER_PID" 2>/dev/null || true
    fi
    rm -f "$PID_FILE"
    echo "$QPKG_NAME failed readiness check; see $LOG_FILE."
    return 1
}

stop_service() {
    if ! is_running; then
        rm -f "$PID_FILE"
        echo "$QPKG_NAME is not running."
        return 0
    fi
    kill "$MANAGER_PID" 2>/dev/null || true
    COUNT=0
    while kill -0 "$MANAGER_PID" 2>/dev/null && [ "$COUNT" -lt 10 ]; do
        sleep 1
        COUNT=$((COUNT + 1))
    done
    if manager_identity_matches && kill -0 "$MANAGER_PID" 2>/dev/null; then
        kill -9 "$MANAGER_PID" 2>/dev/null || true
    fi
    rm -f "$PID_FILE"
    echo "$QPKG_NAME stopped."
}

case "$1" in
  start)
    ENABLED="$(/sbin/getcfg "$QPKG_NAME" Enable -u -d FALSE -f "$CONF")"
    if [ "$ENABLED" != "TRUE" ]; then
        echo "$QPKG_NAME is disabled."
        exit 1
    fi
    start_service
    exit $?
    ;;

  stop)
    stop_service
    exit $?
    ;;

  restart)
    "$0" stop || exit $?
    "$0" start
    exit $?
    ;;
  remove)
    stop_service
    exit $?
    ;;

  *)
    echo "Usage: $0 {start|stop|restart|remove}"
    exit 1
esac
