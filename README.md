# 闪载（FlashGet）

闪载是一款使用 Rust、Tauri 2 和 React 构建的轻量跨平台下载器，支持 HTTP/HTTPS 多连接下载和 BitTorrent 种子下载。

[下载最新版](https://github.com/haoqing-yan/flashget/releases/latest) · [查看构建状态](https://github.com/haoqing-yan/flashget/actions/workflows/release.yml)

## 主要功能

- HTTP/HTTPS Range 分片下载，最多 32 个自适应并发连接
- `.torrent` 文件解析与下载，支持 Tracker、DHT、Peer 和 UPnP
- 添加种子后确认保存目录，并自动创建种子名称文件夹
- 实时显示下载进度、实际速度、Peer 数量和预计完成时间
- 暂停、继续、失败重试和断点续传
- 点击“全部任务”“正在下载”“已完成”切换任务列表
- 删除任务时保留已经下载的文件
- 可选“全部完成后关机”，任务全部成功完成后等待 30 秒再关机
- 持久化 BT 校验结果，避免每次启动都进行完整校验
- 使用硬件优化的 SHA-1 实现加快首次种子校验
- Windows 正式版启动时不显示终端窗口

## 下载安装

在 [GitHub Releases](https://github.com/haoqing-yan/flashget/releases) 下载对应安装包：

- macOS Apple Silicon（M1/M2/M3/M4）：下载 `.dmg`
- Windows x64：下载 `-setup.exe`

当前自动发布 macOS Apple Silicon 和 Windows x64 安装包。Linux 下载核心可编译运行，但暂未提供预构建安装包。

> 当前安装包未进行商业代码签名。macOS 或 Windows 首次启动时可能显示安全提醒，请仅从本仓库 Release 页面下载安装。

## 使用方法

### 下载 HTTP/HTTPS 文件

1. 点击“新建任务”。
2. 填写 HTTP 或 HTTPS 下载链接。
3. 选择保存目录和并发连接数。
4. 点击“立即下载”。

目标服务器需要提供 `Content-Length` 并支持 HTTP Range。如果服务器不支持 Range，多连接分片下载将无法使用。

### 下载种子

1. 点击“添加种子”并选择 `.torrent` 文件。
2. 闪载解析种子后会要求确认保存目录。
3. 确认目录后立即开始校验和下载。

首次导入已有文件时必须进行完整分片校验以保证数据正确。校验结果会保存在应用配置目录中；再次启动或重新添加相同任务时将优先使用快速恢复记录。

BitTorrent 的实际速度取决于可连接 Peer 数量、做种方上传速度、网络 NAT 状态和磁盘性能，宽带带宽本身不能保证达到满速。

### 完成后关机

打开左侧的“全部完成后关机”开关。所有任务都成功完成后，闪载会显示 30 秒提示并关闭计算机；在倒计时内关闭开关即可取消。存在失败或未完成任务时不会触发关机。

## 本地开发

需要 Node.js、Rust，以及对应平台的 [Tauri 系统依赖](https://v2.tauri.app/start/prerequisites/)。

```bash
npm install
npm run tauri dev
```

生产构建：

```bash
npm run build
npm run tauri build
```

代码检查与测试：

```bash
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## 项目结构

```text
src/
├── App.tsx           # 主界面、任务筛选和用户交互
├── styles.css        # 应用界面样式
└── types.ts          # 前端类型

src-tauri/src/
├── lib.rs            # Tauri 启动、插件与命令注册
├── main.rs           # 桌面端入口
├── models.rs         # 前后端共享数据模型
├── state.rs          # 下载任务与 BT Session 运行状态
├── http_download.rs  # HTTP 分片、断点续传与任务控制
├── torrent.rs        # 种子解析、BT 会话与进度同步
└── power.rs          # 下载完成后的系统关机调度
```

## 当前限制

- 暂不支持磁力链接
- 暂不支持浏览器扩展接管下载
- HTTP 下载暂不支持无 Range 服务器的单流降级
- 应用任务列表暂未在重启后完整恢复；BT 分片校验状态已持久化

请只下载和分享你有权使用的内容。
