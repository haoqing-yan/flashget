use crate::{
    http_download::update_progress,
    models::{DownloadTask, TorrentFileInfo, TorrentMetadata},
    power::cancel_pending_shutdown,
    state::{ManagedTask, Manager},
};
use lava_torrent::torrent::v1::Torrent;
use librqbit::api::TorrentIdOrHash;
use librqbit::{
    dht::PersistentDhtConfig, AddTorrent, AddTorrentOptions, Api, Session, SessionOptions,
    SessionPersistenceConfig, TorrentStatsState,
};
use std::{
    collections::HashSet,
    path::PathBuf,
    time::{Duration, Instant},
};
use tauri::{AppHandle, State};
use tokio::fs;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const PUBLIC_TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.demonii.com:1337/announce",
    "udp://tracker.publictracker.xyz:6969/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.wildkat.net:6969/announce",
    "udp://tracker.tryhackx.org:6969/announce",
    "udp://tracker.qu.ax:6969/announce",
    "udp://tracker.peerfect.org:6969/announce",
];

#[tauri::command]
pub(crate) fn parse_torrent(path: String) -> Result<TorrentMetadata, String> {
    let source = PathBuf::from(&path);
    if source.extension().and_then(|extension| extension.to_str()) != Some("torrent") {
        return Err("请选择扩展名为 .torrent 的文件".into());
    }
    let torrent = Torrent::read_from_file(&source)
        .map_err(|error| format!("无效或损坏的种子文件：{error}"))?;
    if torrent.length < 0 || torrent.piece_length <= 0 {
        return Err("种子中包含无效的文件大小或分片大小".into());
    }

    let files = match &torrent.files {
        Some(items) => items
            .iter()
            .map(|file| {
                if file.length < 0 {
                    return Err("种子文件列表中包含无效大小".to_string());
                }
                Ok(TorrentFileInfo {
                    path: file.path.to_string_lossy().into_owned(),
                    length: file.length as u64,
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => vec![TorrentFileInfo {
            path: torrent.name.clone(),
            length: torrent.length as u64,
        }],
    };

    let mut trackers = torrent.announce.iter().cloned().collect::<Vec<_>>();
    if let Some(tiers) = &torrent.announce_list {
        for tracker in tiers.iter().flatten() {
            if !trackers.contains(tracker) {
                trackers.push(tracker.clone());
            }
        }
    }

    Ok(TorrentMetadata {
        name: torrent.name.clone(),
        info_hash: torrent.info_hash(),
        total_size: torrent.length as u64,
        piece_length: torrent.piece_length as u64,
        piece_count: torrent.pieces.len(),
        trackers,
        files,
        source_path: source.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub(crate) async fn create_torrent_download(
    app: AppHandle,
    manager: State<'_, Manager>,
    path: String,
    destination: Option<String>,
) -> Result<DownloadTask, String> {
    let meta = parse_torrent(path.clone())?;
    let base_dir = destination
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .or_else(dirs::download_dir)
        .ok_or("无法确定下载目录")?;
    let folder_name = safe_folder_name(&meta.name, &meta.info_hash);
    let target = base_dir.join(folder_name);
    fs::create_dir_all(&target)
        .await
        .map_err(|error| format!("无法创建种子下载文件夹：{error}"))?;
    let session = get_or_create_session(&manager, target.clone()).await?;
    let torrent_id = TorrentIdOrHash::try_from(meta.info_hash.as_str())
        .map_err(|error| format!("种子哈希无效：{error}"))?;
    if session.get(torrent_id).is_some() {
        let details = Api::new(session.clone(), None)
            .api_torrent_details(torrent_id)
            .map_err(|error| format!("读取已有 BT 任务失败：{error}"))?;
        if !paths_match(&details.output_folder, &target) {
            session
                .delete(torrent_id, false)
                .await
                .map_err(|error| format!("切换种子下载目录失败：{error}"))?;
            remove_torrent_from_manager(&manager, meta.info_hash.as_str()).await;
        }
    }
    let bytes = fs::read(&path).await.map_err(|error| error.to_string())?;
    let handle = session
        .add_torrent(
            AddTorrent::from_bytes(bytes),
            Some(AddTorrentOptions {
                overwrite: true,
                output_folder: Some(target.to_string_lossy().into_owned()),
                force_tracker_interval: Some(Duration::from_secs(10 * 60)),
                ..Default::default()
            }),
        )
        .await
        .map_err(|error| format!("启动 BT 下载失败：{error}"))?
        .into_handle()
        .ok_or("无法创建 BT 下载会话")?;

    let id = Uuid::new_v4().to_string();
    let task = DownloadTask {
        id: id.clone(),
        url: path,
        file_name: meta.name,
        destination: target.to_string_lossy().into_owned(),
        downloaded: 0,
        total: meta.total_size,
        speed: 0,
        eta_seconds: None,
        peers_connected: 0,
        peers_seen: 0,
        status: "downloading".into(),
        error: None,
    };
    manager.tasks.write().await.insert(
        id.clone(),
        ManagedTask {
            info: task.clone(),
            connections: 0,
            cancellation: CancellationToken::new(),
        },
    );
    cancel_pending_shutdown(&manager);
    manager.bt.write().await.insert(id.clone(), handle.clone());

    let manager = manager.inner().clone();
    tauri::async_runtime::spawn(async move {
        let mut last_fetched: Option<(u64, Instant)> = None;
        let mut smoothed_speed = 0;
        loop {
            let stats = handle.stats();
            let now = Instant::now();
            let live = stats.live.as_ref();
            let engine_speed = live
                .map(|live| (live.download_speed.mbps * 1024.0 * 1024.0) as u64)
                .unwrap_or(0);
            let fetched = live.map(|live| live.snapshot.fetched_bytes).unwrap_or(0);
            let measured_speed = if live.is_some() {
                let measured = last_fetched
                    .map(|(previous, sampled_at)| {
                        let elapsed = now.duration_since(sampled_at).as_secs_f64();
                        if elapsed > 0.0 {
                            (fetched.saturating_sub(previous) as f64 / elapsed) as u64
                        } else {
                            0
                        }
                    })
                    .unwrap_or(0);
                last_fetched = Some((fetched, now));
                measured
            } else {
                last_fetched = None;
                smoothed_speed = 0;
                0
            };
            smoothed_speed = smooth_speed(smoothed_speed, measured_speed.max(engine_speed));
            let initializing = matches!(stats.state, TorrentStatsState::Initializing);
            let speed = if initializing { 0 } else { smoothed_speed };
            let (peers_connected, peers_seen) = stats
                .live
                .as_ref()
                .map(|live| {
                    let peers = &live.snapshot.peer_stats;
                    (peers.live, peers.seen)
                })
                .unwrap_or_default();
            if let Some(task) = manager.tasks.write().await.get_mut(&id) {
                task.info.peers_connected = peers_connected;
                task.info.peers_seen = peers_seen;
            }
            let status = if stats.finished {
                "completed"
            } else if stats.error.is_some() {
                "failed"
            } else if handle.is_paused() {
                "paused"
            } else if initializing {
                "checking"
            } else {
                "downloading"
            };
            update_progress(
                &app,
                &manager,
                &id,
                stats.progress_bytes,
                stats.total_bytes,
                speed,
                status,
            )
            .await;
            if stats.finished || stats.error.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
    Ok(task)
}

async fn get_or_create_session(
    manager: &Manager,
    target: PathBuf,
) -> Result<std::sync::Arc<Session>, String> {
    let mut current = manager.bt_session.lock().await;
    if let Some(session) = current.as_ref() {
        return Ok(session.clone());
    }
    let config_dir = dirs::config_dir()
        .map(|path| path.join("flashget"))
        .ok_or("无法确定应用配置目录")?;
    fs::create_dir_all(&config_dir)
        .await
        .map_err(|error| format!("无法创建应用配置目录：{error}"))?;
    let options = SessionOptions {
        disable_dht_persistence: false,
        dht_config: Some(PersistentDhtConfig {
            dump_interval: Some(Duration::from_secs(5 * 60)),
            config_filename: Some(config_dir.join("dht.json")),
        }),
        listen_port_range: Some(49152..49162),
        enable_upnp_port_forwarding: true,
        fastresume: true,
        persistence: Some(SessionPersistenceConfig::Json {
            folder: Some(config_dir.join("session")),
        }),
        defer_writes_up_to: Some(256),
        concurrent_init_limit: Some(4),
        trackers: public_trackers(),
        ..Default::default()
    };
    let session = Session::new_with_opts(target, options)
        .await
        .map_err(|error| format!("初始化 BT 网络失败：{error}"))?;
    *current = Some(session.clone());
    Ok(session)
}

fn public_trackers() -> HashSet<url::Url> {
    PUBLIC_TRACKERS
        .iter()
        .filter_map(|tracker| url::Url::parse(tracker).ok())
        .collect()
}

fn smooth_speed(previous: u64, current: u64) -> u64 {
    match (previous, current) {
        (0, current) => current,
        (previous, 0) => previous.saturating_mul(3) / 4,
        (previous, current) => ((previous as f64 * 0.65) + (current as f64 * 0.35)).round() as u64,
    }
}

async fn remove_torrent_from_manager(manager: &Manager, info_hash: &str) {
    let ids = {
        let torrents = manager.bt.read().await;
        torrents
            .iter()
            .filter(|(_, handle)| handle.info_hash().as_string() == info_hash)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>()
    };
    if ids.is_empty() {
        return;
    }
    let mut torrents = manager.bt.write().await;
    let mut tasks = manager.tasks.write().await;
    for id in ids {
        torrents.remove(&id);
        if let Some(task) = tasks.remove(&id) {
            task.cancellation.cancel();
        }
    }
}

fn paths_match(existing: &str, requested: &PathBuf) -> bool {
    let existing = PathBuf::from(existing);
    match (existing.canonicalize(), requested.canonicalize()) {
        (Ok(existing), Ok(requested)) => existing == requested,
        _ => {
            #[cfg(target_os = "windows")]
            {
                existing
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&requested.to_string_lossy())
            }
            #[cfg(not(target_os = "windows"))]
            {
                existing == *requested
            }
        }
    }
}

fn safe_folder_name(name: &str, info_hash: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect();
    let sanitized = sanitized.trim().trim_matches(['.', ' ']);
    if sanitized.is_empty() {
        format!("torrent-{}", &info_hash[..info_hash.len().min(8)])
    } else {
        sanitized.chars().take(120).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{public_trackers, safe_folder_name, smooth_speed, PUBLIC_TRACKERS};

    #[test]
    fn sanitizes_cross_platform_folder_names() {
        assert_eq!(
            safe_folder_name("movie: part/one?.mkv", "1234567890"),
            "movie_ part_one_.mkv"
        );
    }

    #[test]
    fn falls_back_when_name_has_no_usable_characters() {
        assert_eq!(safe_folder_name("... ", "1234567890"), "torrent-12345678");
    }

    #[test]
    fn smooths_measured_download_speed() {
        assert_eq!(smooth_speed(0, 10_000), 10_000);
        assert_eq!(smooth_speed(10_000, 20_000), 13_500);
        assert_eq!(smooth_speed(10_000, 0), 7_500);
    }

    #[test]
    fn parses_all_public_trackers() {
        assert_eq!(public_trackers().len(), PUBLIC_TRACKERS.len());
    }
}
