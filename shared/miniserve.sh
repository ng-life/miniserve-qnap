#!/bin/sh
CONF=/etc/config/qpkg.conf
# 动态获取安装路径
QPKG_ROOT=`/sbin/getcfg miniserve Install_Path -f ${CONF}`

case "$1" in
  start)
    ENABLED=$(/sbin/getcfg miniserve Enable -u -d FALSE -f ${CONF})
    if [ "$ENABLED" != "TRUE" ]; then
        echo "miniserve is disabled."
        exit 1
    fi

    # 启动命令说明：
    # -p 8080: 监听端口
    # -u: 允许上传
    # -a admin:admin: 设置简单的认证 (可选)
    # /share/Public: 默认分享的目录
    nohup ${QPKG_ROOT}/miniserve -p 8080 --unlimited-max-file-size -u /share/Public > /dev/null 2>&1 &
    
    # 记录 PID 以便精确停止 (可选)
    echo $! > /var/run/miniserve.pid
    ;;

  stop)
    echo "Stopping miniserve..."
    # 尝试通过 PID 停止，如果没有则通过 killall
    if [ -f /var/run/miniserve.pid ]; then
        kill $(cat /var/run/miniserve.pid)
        rm /var/run/miniserve.pid
    else
        killall -9 miniserve
    fi
    ;;

  restart)
    $0 stop
    $0 start
    ;;

  *)
    echo "Usage: $0 {start|stop|restart}"
    exit 1
esac

exit 0
