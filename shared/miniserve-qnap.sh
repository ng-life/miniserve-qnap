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
export QNAP_QPKG="$QPKG_NAME"

is_running() {
    [ -f "$PID_FILE" ] || return 1
    PID="$(cat "$PID_FILE" 2>/dev/null)"
    [ -n "$PID" ] && kill -0 "$PID" 2>/dev/null
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
        --miniserve "$MINISERVE" \
        --listen "0.0.0.0:8090" \
        >>"$LOG_FILE" 2>&1 &
    PID=$!
    echo "$PID" > "$PID_FILE"
    chmod 600 "$PID_FILE"
    COUNT=0
    while [ "$COUNT" -lt 10 ]; do
        if kill -0 "$PID" 2>/dev/null; then
            sleep 1
            if kill -0 "$PID" 2>/dev/null; then
                echo "$QPKG_NAME started with PID $PID."
                return 0
            fi
        fi
        COUNT=$((COUNT + 1))
    done
    rm -f "$PID_FILE"
    echo "$QPKG_NAME failed to start; see $LOG_FILE."
    return 1
}

stop_service() {
    if ! is_running; then
        rm -f "$PID_FILE"
        echo "$QPKG_NAME is not running."
        return 0
    fi
    PID="$(cat "$PID_FILE")"
    kill "$PID" 2>/dev/null || true
    COUNT=0
    while kill -0 "$PID" 2>/dev/null && [ "$COUNT" -lt 10 ]; do
        sleep 1
        COUNT=$((COUNT + 1))
    done
    if kill -0 "$PID" 2>/dev/null; then
        kill -9 "$PID" 2>/dev/null || true
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
    ;;

  stop)
    stop_service
    ;;

  restart)
    $0 stop
    $0 start
    ;;
  remove)
    stop_service
    ;;

  *)
    echo "Usage: $0 {start|stop|restart|remove}"
    exit 1
esac

exit 0
