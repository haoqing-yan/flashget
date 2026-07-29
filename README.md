# 闪载（FlashGet）

一个使用 Rust、Tauri 2 和 React 构建的跨平台桌面下载器 MVP。

## 第一版功能

- HTTP/HTTPS 下载
- 1–16 个并发 Range 分片
- 实时进度、速度与状态展示
- 暂停、继续和失败重试
- 分片临时文件保留，可断点续传
- 选择、解析并下载 `.torrent` 种子文件
- 展示 Info Hash、文件列表、Tracker 与分片信息
- Windows、macOS、Linux 共用下载核心

服务器需要提供 `Content-Length` 并支持 HTTP Range。BitTorrent 下载由嵌入式 librqbit 引擎提供 Tracker、DHT、Peer 连接和分片校验。磁力链接、浏览器扩展、任务历史持久化和无 Range 单流降级将在后续版本中实现。

## 本地开发

需要 Node.js、Rust，以及对应平台的 Tauri 系统依赖。

```bash
npm install
npm run tauri dev
```

仅构建前端：

```bash
npm run build
```

检查 Rust 下载核心：

```bash
cd src-tauri
cargo check
```

## Rust 后端结构

```text
src-tauri/src/
├── lib.rs            # Tauri 启动、插件和命令注册
├── main.rs           # 桌面端入口
├── models.rs         # 前后端共享的数据模型
├── state.rs          # 下载任务、BT Session 等运行状态
├── http_download.rs  # HTTP 分片、断点续传和任务控制
└── torrent.rs        # 种子解析、BT 会话和进度同步
```

协议实现不再堆积在 `lib.rs`。后续磁力链接、持久化和限速功能可以继续按模块扩展。

## 打包

```bash
npm run tauri build
```

Tauri 通常应在各目标操作系统上分别构建安装包。也可以使用 CI 同时产出 Windows、macOS 与 Linux 版本。
