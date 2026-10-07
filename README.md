# Miniserve for QNAP

[![Build x86_64 QPKG](https://github.com/ng-life/miniserve-qnap/actions/workflows/main.yml/badge.svg)](https://github.com/ng-life/miniserve-qnap/actions/workflows/main.yml)
[![GitHub release](https://img.shields.io/github/v/release/ng-life/miniserve-qnap)](https://github.com/ng-life/miniserve-qnap/releases/latest)

面向 QNAP QTS / QuTS hero x86_64 NAS 的 Miniserve QPKG。应用内置：

- 官方 Miniserve 0.35.0 `x86_64-unknown-linux-musl` 静态二进制；
- 静态链接的 Rust 管理服务；
- 中文 Web 控制台，管理共享目录、监听地址、端口、标题、路由前缀、上传、创建目录、隐藏文件、符号链接、HTTP Basic 认证、主题、排序、索引文件和美观 URL；
- 配置校验、原子写入、保存后重启和最近 100 条运行日志；
- 复用 QTS 系统认证的管理控制台、Miniserve 就绪检测和安全的 PID 身份校验。
- 使用 Miniserve 0.35.0 内嵌的官方标识作为 QTS App Center 图标，并提供 QNAP 所需的 64 px、80 px 和禁用状态版本。

## 端口

- `8090`：仅监听 NAS 本机，作为 QTS HTTP 代理的管理后端；
- `18080`：默认文件共享端口，可在控制台修改。

管理控制台通过 QTS HTTP 代理挂载在 `/miniserve`，复用 QNAP 系统登录认证，不再要求独立的管理用户名和密码。管理服务强制只监听 `127.0.0.1:8090`，即使使用 `--listen` 参数指定其他地址也会拒绝启动；外部访问须通过 QTS 代理路径。请保持 QTS 对该代理路径的系统认证和管理员访问控制，不要将后端通过其他未认证的代理或端口转发暴露出去。是否使用 HTTPS 取决于 QTS 管理界面的访问方式。

页面在 `/miniserve` 和 `/miniserve/` 下均使用 `/miniserve/api/...` 请求；后端同时兼容 QTS 转发时保留或剥离路径前缀。健康检查仍可在本机使用 `/healthz`。

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
scripts/verify-qpkg.sh build/miniserve-qnap_1.0.7_x86_64.qpkg
```

构建结果位于 `build/`。可以在 QTS 的 App Center 中选择“手动安装”，上传生成的 `.qpkg`。

GitHub Actions 会在每次推送和 Pull Request 时执行单元测试、QTS 生命周期脚本兼容性检查、管理 API/文件共享认证冒烟测试、HTTP 代理元数据检查、严格 QPKG 构建及包清单、属主和权限审计，然后上传 x86_64 Artifact。推送与 `QPKG_VER` 对应的标签（例如 `v1.0.7`）时，会自动创建 GitHub Release 并附加 `.qpkg` 与 MD5 文件。

## 下载与安装

从 [最新 Release](https://github.com/ng-life/miniserve-qnap/releases/latest) 下载 `miniserve-qnap_*_x86_64.qpkg`，然后在 QTS App Center 中选择“手动安装”。

## 安装后目录

QPKG 安装目录中的 `var/config.json` 和 `var/auth.txt` 分别保存运行配置和可选的文件共享凭据；旧版本的 `var/admin-auth.txt` 不再使用；卸载应用时会随应用一并删除。默认共享目录为 `/share/Public`，默认关闭上传并禁止跟随符号链接。

发布的 QPKG 当前没有 QNAP 官方代码签名，QTS 手动安装时可能显示第三方/未签名提示。请从本仓库 Release 下载并核对随附校验值。

## 上游与许可

- Miniserve: <https://github.com/svenstaro/miniserve>（MIT）
- QDK: <https://github.com/qnap-dev/QDK>（GPL）

本项目的管理服务和界面采用 MIT 许可。
