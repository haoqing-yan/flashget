import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import appIcon from "./assets/app-icon.png";
import {
  CheckCircle2,
  Download,
  FolderOpen,
  FileArchive,
  Gauge,
  HardDriveDownload,
  Pause,
  Play,
  Plus,
  RotateCcw,
  X
} from "lucide-react";
import type { DownloadTask, ProgressPayload, TaskStatus, TorrentMetadata } from "./types";

const inTauri = "__TAURI_INTERNALS__" in window;

function formatBytes(value: number) {
  if (!value) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  return `${(value / 1024 ** index).toFixed(index > 1 ? 1 : 0)} ${units[index]}`;
}

function statusText(status: TaskStatus) {
  return {
    queued: "等待中",
    checking: "正在校验",
    downloading: "下载中",
    paused: "已暂停",
    completed: "已完成",
    failed: "下载失败"
  }[status];
}

function formatEta(seconds: number) {
  if (seconds < 60) return `${Math.max(1, Math.round(seconds))} 秒`;
  if (seconds < 3600) return `${Math.ceil(seconds / 60)} 分钟`;
  if (seconds < 86400) {
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.ceil((seconds % 3600) / 60);
    return `${hours} 小时${minutes ? ` ${minutes} 分钟` : ""}`;
  }
  return `${Math.floor(seconds / 86400)} 天 ${Math.ceil((seconds % 86400) / 3600)} 小时`;
}

function completionTime(seconds: number) {
  return new Date(Date.now() + seconds * 1000).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit"
  });
}

function App() {
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [showDialog, setShowDialog] = useState(false);
  const [url, setUrl] = useState("");
  const [destination, setDestination] = useState("");
  const [connections, setConnections] = useState(16);
  const [parsingTorrent, setParsingTorrent] = useState(false);

  useEffect(() => {
    if (!inTauri) return;
    invoke<DownloadTask[]>("list_tasks").then(setTasks).catch(console.error);
    const unlisten = listen<ProgressPayload>("download-progress", ({ payload }) => {
      setTasks((current) =>
        current.map((task) =>
          task.id === payload.id ? { ...task, ...payload } : task
        )
      );
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const stats = useMemo(() => {
    const active = tasks.filter((task) => task.status === "downloading" || task.status === "checking");
    return {
      active: active.length,
      completed: tasks.filter((task) => task.status === "completed").length,
      speed: active.reduce((sum, task) => sum + task.speed, 0)
    };
  }, [tasks]);

  async function createTask(event: React.FormEvent) {
    event.preventDefault();
    if (!url.trim()) return;
    if (!inTauri) {
      alert("请在 Tauri 桌面应用中运行以创建真实下载任务。");
      return;
    }
    try {
      const task = await invoke<DownloadTask>("create_download", {
        url: url.trim(),
        destination: destination.trim() || null,
        connections
      });
      setTasks((current) => [task, ...current]);
      setUrl("");
      setShowDialog(false);
    } catch (error) {
      alert(String(error));
    }
  }

  async function control(id: string, action: "pause" | "resume") {
    try {
      await invoke(action === "pause" ? "pause_download" : "resume_download", { id });
    } catch (error) {
      alert(String(error));
    }
  }

  async function chooseDownloadDirectory() {
    if (!inTauri) {
      alert("请在 Tauri 桌面应用中选择下载目录。");
      return;
    }
    const path = await open({
      title: "选择下载目录",
      multiple: false,
      directory: true
    });
    if (path) setDestination(path);
  }

  async function chooseTorrent() {
    if (!inTauri) {
      alert("请在 Tauri 桌面应用中选择种子文件。");
      return;
    }
    const path = await open({
      title: "选择种子文件",
      multiple: false,
      directory: false,
      filters: [{ name: "BitTorrent 种子", extensions: ["torrent"] }]
    });
    if (!path) return;
    setParsingTorrent(true);
    try {
      await invoke<TorrentMetadata>("parse_torrent", { path });
      const task = await invoke<DownloadTask>("create_torrent_download", {
        path,
        destination: destination.trim() || null
      });
      setTasks((current) => [task, ...current]);
    } catch (error) {
      alert(`种子解析失败：${String(error)}`);
    } finally {
      setParsingTorrent(false);
    }
  }

  return (
    <main className="shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">
            <img src={appIcon} alt="闪载 Logo" />
          </div>
          <div><strong>闪载</strong><span>FlashGet</span></div>
        </div>
        <nav>
          <button className="nav-item active"><HardDriveDownload size={19} />全部任务<span>{tasks.length}</span></button>
          <button className="nav-item"><Gauge size={19} />正在下载<span>{stats.active}</span></button>
          <button className="nav-item"><CheckCircle2 size={19} />已完成<span>{stats.completed}</span></button>
        </nav>
        <div className="sidebar-foot">
          <p>当前总速度</p>
          <strong>{formatBytes(stats.speed)}/s</strong>
          <small>多连接并行下载已启用</small>
        </div>
      </aside>

      <section className="content">
        <header>
          <div>
            <h1>下载任务</h1>
            <p>高速、安静地完成每一次下载</p>
          </div>
          <div className="header-actions">
            <button className="secondary" onClick={chooseDownloadDirectory}><FolderOpen size={19} />下载目录</button>
            <button className="secondary" onClick={chooseTorrent} disabled={parsingTorrent}><FileArchive size={19} />{parsingTorrent ? "正在添加…" : "添加种子"}</button>
            <button className="primary" onClick={() => setShowDialog(true)}><Plus size={19} />新建任务</button>
          </div>
        </header>

        {tasks.length === 0 ? (
          <div className="empty">
            <div className="empty-icon"><Download size={38} /></div>
            <h2>还没有下载任务</h2>
            <p>粘贴一个 HTTP 或 HTTPS 链接开始下载</p>
            <button className="primary" onClick={() => setShowDialog(true)}><Plus size={19} />添加第一个任务</button>
          </div>
        ) : (
          <div className="task-list">
            {tasks.map((task) => {
              const progress = task.total ? Math.min(100, task.downloaded / task.total * 100) : 0;
              const active = task.status === "downloading" || task.status === "checking";
              return (
                <article className="task" key={task.id}>
                  <div className={`file-icon ${task.status}`}><Download size={22} /></div>
                  <div className="task-main">
                    <div className="task-title">
                      <strong>{task.fileName}</strong>
                      <span className={task.status}>{statusText(task.status)}</span>
                    </div>
                    <div className="progress"><i style={{ width: `${progress}%` }} /></div>
                    <div className="task-meta">
                      <span>{formatBytes(task.downloaded)} / {formatBytes(task.total)}</span>
                      <span>{progress.toFixed(1)}%</span>
                      {task.peersSeen > 0 && <span>Peer {task.peersConnected} / 已发现 {task.peersSeen}</span>}
                      {task.status === "downloading" && <span className="speed">{formatBytes(task.speed)}/s</span>}
                      {task.status === "downloading" && task.etaSeconds != null && (
                        <span className="eta">预计 {completionTime(task.etaSeconds)} 完成 · 剩余 {formatEta(task.etaSeconds)}</span>
                      )}
                      {task.error && <span className="error">{task.error}</span>}
                    </div>
                  </div>
                  <button className="icon-button" title={active ? "暂停" : "继续"}
                    disabled={task.status === "completed"}
                    onClick={() => control(task.id, active ? "pause" : "resume")}>
                    {active ? <Pause size={18} /> : task.status === "failed" ? <RotateCcw size={18} /> : <Play size={18} />}
                  </button>
                </article>
              );
            })}
          </div>
        )}
      </section>

      {showDialog && (
        <div className="backdrop" onMouseDown={() => setShowDialog(false)}>
          <form className="dialog" onSubmit={createTask} onMouseDown={(event) => event.stopPropagation()}>
            <div className="dialog-head"><div><h2>新建下载任务</h2><p>支持 HTTP/HTTPS 与断点续传</p></div><button type="button" className="icon-button" onClick={() => setShowDialog(false)}><X size={19} /></button></div>
            <label>下载链接<textarea autoFocus value={url} onChange={(event) => setUrl(event.target.value)} placeholder="https://example.com/file.zip" required /></label>
            <label>保存目录（留空使用系统下载目录）<div className="input-icon directory-input"><FolderOpen size={18} /><input value={destination} onChange={(event) => setDestination(event.target.value)} placeholder="默认下载目录" /><button type="button" onClick={chooseDownloadDirectory}>选择</button></div></label>
            <label>并发连接数 <strong>{connections}</strong><input type="range" min="1" max="32" value={connections} onChange={(event) => setConnections(Number(event.target.value))} /></label>
            <div className="dialog-actions"><button type="button" className="secondary" onClick={() => setShowDialog(false)}>取消</button><button className="primary" type="submit"><Download size={18} />立即下载</button></div>
          </form>
        </div>
      )}

    </main>
  );
}

export default App;
