# Miniserve for QNAP

[![Build x86_64 QPKG](https://github.com/ng-life/miniserve-qnap/actions/workflows/main.yml/badge.svg)](https://github.com/ng-life/miniserve-qnap/actions/workflows/main.yml)
[![GitHub release](https://img.shields.io/github/v/release/ng-life/miniserve-qnap)](https://github.com/ng-life/miniserve-qnap/releases/latest)

面向 QNAP QTS / QuTS hero x86_64 NAS 的 Miniserve QPKG。应用内置：

- 官方 Miniserve 0.35.0 `x86_64-unknown-linux-musl` 静态二进制；
- 静态链接的 Rust 管理服务；
- 中文 Web 控制台，管理共享目录、监听地址、端口、标题、路由前缀、上传、创建目录、隐藏文件、符号链接、HTTP Basic 认证、主题、排序、索引文件和美观 URL；
- 配置校验、原子写入、保存后重启和最近 100 条运行日志。
- 使用 Miniserve 0.35.0 内嵌的官方标识作为 QTS App Center 图标，并提供 QNAP 所需的 64 px、80 px 和禁用状态版本。

## 端口

- `8090`：管理控制台，由 QTS App Center 图标打开；
- `8080`：默认文件共享端口，可在控制台修改。

管理控制台监听 NAS 的所有接口，仅建议在可信局域网使用。文件共享的用户名和密码通过控制台配置，密码保存在应用私有目录，文件权限为 `0600`，且不会由状态 API 返回。

## 本地构建

需要 QDK 2.5.3、Rust 和 musl 工具链：

```bash
sudo apt install musl-tools
rustup target add x86_64-unknown-linux-musl
cargo test
cargo build --release --target x86_64-unknown-linux-musl
install -m 0755 target/x86_64-unknown-linux-musl/release/miniserve-qnap-manager \
  x86_64/bin/miniserve-qnap-manager
qbuild --build-arch x86_64 --strict
```

构建结果位于 `build/`。可以在 QTS 的 App Center 中选择“手动安装”，上传生成的 `.qpkg`。

GitHub Actions 会在每次推送和 Pull Request 时构建、校验并上传 x86_64 QPKG Artifact。推送与 `QPKG_VER` 对应的标签（例如 `v1.0.1`）时，会自动创建 GitHub Release 并附加 `.qpkg` 与 MD5 文件。

## 下载与安装

从 [最新 Release](https://github.com/ng-life/miniserve-qnap/releases/latest) 下载 `miniserve-qnap_*_x86_64.qpkg`，然后在 QTS App Center 中选择“手动安装”。

## 安装后目录

QPKG 安装目录中的 `var/config.json` 和 `var/auth.txt` 保存运行配置；卸载应用时会随应用一并删除。默认共享目录为 `/share/Public`，默认关闭上传并禁止跟随符号链接。

## 上游与许可

- Miniserve: <https://github.com/svenstaro/miniserve>（MIT）
- QDK: <https://github.com/qnap-dev/QDK>（GPL）

本项目的管理服务和界面采用 MIT 许可。
