# Miniserve for QNAP

[![Build x86_64 QPKG](https://github.com/ng-life/miniserve-qnap/actions/workflows/main.yml/badge.svg)](https://github.com/ng-life/miniserve-qnap/actions/workflows/main.yml)
[![GitHub release](https://img.shields.io/github/v/release/ng-life/miniserve-qnap)](https://github.com/ng-life/miniserve-qnap/releases/latest)

面向 QNAP QTS / QuTS hero x86_64 NAS 的 Miniserve QPKG。应用内置：

- 官方 Miniserve 0.35.0 `x86_64-unknown-linux-musl` 静态二进制；
- 静态链接的 Rust 管理服务；
- 中文 Web 控制台，管理共享目录、监听地址、端口、标题、路由前缀、上传、创建目录、隐藏文件、符号链接、HTTP Basic 认证、主题、排序、索引文件和美观 URL；
- 配置校验、原子写入、保存后重启和最近 100 条运行日志；
- 独立的管理控制台认证、Miniserve 就绪检测和安全的 PID 身份校验。
- 使用 Miniserve 0.35.0 内嵌的官方标识作为 QTS App Center 图标，并提供 QNAP 所需的 64 px、80 px 和禁用状态版本。

## 端口

- `8090`：管理控制台，由 QTS App Center 图标打开；
- `8080`：默认文件共享端口，可在控制台修改。

管理控制台监听 NAS 的所有接口，并强制使用独立的 HTTP Basic 认证。首次启动会生成随机管理密码；用户名为 `admin`。通过 SSH 登录 NAS 后执行：

```sh
QPKG_ROOT="$(/sbin/getcfg miniserve-qnap Install_Path -f /etc/config/qpkg.conf)"
sudo cat "$QPKG_ROOT/var/admin-auth.txt"
```

文件内容格式为 `用户名:密码`，权限为 `0600`。在 QTS 中点击应用图标后，浏览器会显示登录提示。管理流量当前使用 HTTP，请只在可信局域网或受保护的 VPN 内访问，不要把 `8090` 暴露到互联网。

文件共享使用另一套、可选的用户名和密码，通过控制台配置。文件共享密码同样以 `0600` 保存，且状态 API 永不返回存储的密码。

## 本地构建

需要 QDK 2.5.3、Rust、`fakeroot` 和 musl 工具链：

```bash
sudo apt install fakeroot musl-tools
rustup target add x86_64-unknown-linux-musl
cargo test
cargo build --release --target x86_64-unknown-linux-musl
install -m 0755 target/x86_64-unknown-linux-musl/release/miniserve-qnap-manager \
  x86_64/bin/miniserve-qnap-manager
fakeroot qbuild --build-arch x86_64 --strict
scripts/verify-qpkg.sh build/miniserve-qnap_1.0.4_x86_64.qpkg
```

构建结果位于 `build/`。可以在 QTS 的 App Center 中选择“手动安装”，上传生成的 `.qpkg`。

GitHub Actions 会在每次推送和 Pull Request 时执行单元测试、QTS 生命周期脚本兼容性检查、认证 API/Miniserve 冒烟测试、严格 QPKG 构建及包清单、属主和权限审计，然后上传 x86_64 Artifact。推送与 `QPKG_VER` 对应的标签（例如 `v1.0.4`）时，会自动创建 GitHub Release 并附加 `.qpkg` 与 MD5 文件。

## 下载与安装

从 [最新 Release](https://github.com/ng-life/miniserve-qnap/releases/latest) 下载 `miniserve-qnap_*_x86_64.qpkg`，然后在 QTS App Center 中选择“手动安装”。

## 安装后目录

QPKG 安装目录中的 `var/config.json`、`var/auth.txt` 和 `var/admin-auth.txt` 分别保存运行配置、可选的文件共享凭据和强制启用的管理凭据；卸载应用时会随应用一并删除。默认共享目录为 `/share/Public`，默认关闭上传并禁止跟随符号链接。

发布的 QPKG 当前没有 QNAP 官方代码签名，QTS 手动安装时可能显示第三方/未签名提示。请从本仓库 Release 下载并核对随附校验值。

## 上游与许可

- Miniserve: <https://github.com/svenstaro/miniserve>（MIT）
- QDK: <https://github.com/qnap-dev/QDK>（GPL）

本项目的管理服务和界面采用 MIT 许可。
