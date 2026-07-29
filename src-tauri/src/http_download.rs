use crate::{
    models::DownloadTask,
    state::{ManagedTask, Manager},
};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, RANGE};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, State};
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
};
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;

#[tauri::command]
pub(crate) async fn list_tasks(manager: State<'_, Manager>) -> Result<Vec<DownloadTask>, String> {
    let tasks = manager.tasks.read().await;
    let mut result: Vec<_> = tasks.values().map(|task| task.info.clone()).collect();
    result.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(result)
}

#[tauri::command]
pub(crate) async fn create_download(
    app: AppHandle,
    manager: State<'_, Manager>,
    url: String,
    destination: Option<String>,
    connections: usize,
) -> Result<DownloadTask, String> {
    let parsed = Url::parse(&url).map_err(|_| "下载链接格式不正确")?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("第一版仅支持 HTTP 和 HTTPS 链接".into());
    }
    let client = http_client()?;
    let response = client
        .head(parsed.clone())
        .send()
        .await
        .map_err(friendly_error)?;
    if !response.status().is_success() {
        return Err(format!("服务器返回 {}", response.status()));
    }
    let total = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let file_name = response
        .headers()
        .get(CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .and_then(file_name_from_disposition)
        .or_else(|| {
            parsed
                .path_segments()?
                .next_back()
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "download.bin".into());
    let target_dir = destination
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .or_else(dirs::download_dir)
        .ok_or("无法确定系统下载目录，请手动填写保存目录")?;
    fs::create_dir_all(&target_dir)
        .await
        .map_err(|error| format!("无法创建保存目录：{error}"))?;

    let id = Uuid::new_v4().to_string();
    let task = DownloadTask {
        id: id.clone(),
        url,
        file_name,
        destination: target_dir.to_string_lossy().into_owned(),
        downloaded: 0,
        total,
        speed: 0,
        eta_seconds: None,
        status: "downloading".into(),
        error: None,
    };
    let cancellation = CancellationToken::new();
    manager.tasks.write().await.insert(
        id.clone(),
        ManagedTask {
            info: task.clone(),
            connections: connections.clamp(1, 16),
            cancellation: cancellation.clone(),
        },
    );
    spawn_download(app, manager.inner().clone(), id, cancellation);
    Ok(task)
}

#[tauri::command]
pub(crate) async fn pause_download(manager: State<'_, Manager>, id: String) -> Result<(), String> {
    if let Some(handle) = manager.bt.read().await.get(&id).cloned() {
        let session = manager
            .bt_session
            .lock()
            .await
            .clone()
            .ok_or("BT 会话未启动")?;
        session
            .pause(&handle)
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    let mut tasks = manager.tasks.write().await;
    let task = tasks.get_mut(&id).ok_or("找不到下载任务")?;
    task.cancellation.cancel();
    task.info.status = "paused".into();
    task.info.speed = 0;
    task.info.eta_seconds = None;
    Ok(())
}

#[tauri::command]
pub(crate) async fn resume_download(
    app: AppHandle,
    manager: State<'_, Manager>,
    id: String,
) -> Result<(), String> {
    if let Some(handle) = manager.bt.read().await.get(&id).cloned() {
        let session = manager
            .bt_session
            .lock()
            .await
            .clone()
            .ok_or("BT 会话未启动")?;
        session
            .unpause(&handle)
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    let cancellation = CancellationToken::new();
    {
        let mut tasks = manager.tasks.write().await;
        let task = tasks.get_mut(&id).ok_or("找不到下载任务")?;
        if task.info.status == "completed" || task.info.status == "downloading" {
            return Ok(());
        }
        task.info.status = "downloading".into();
        task.info.error = None;
        task.cancellation = cancellation.clone();
    }
    spawn_download(app, manager.inner().clone(), id, cancellation);
    Ok(())
}

fn spawn_download(app: AppHandle, manager: Manager, id: String, cancellation: CancellationToken) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_download(&app, &manager, &id, &cancellation).await {
            if cancellation.is_cancelled() {
                return;
            }
            let payload = {
                let mut tasks = manager.tasks.write().await;
                tasks.get_mut(&id).map(|task| {
                    task.info.status = "failed".into();
                    task.info.speed = 0;
                    task.info.eta_seconds = None;
                    task.info.error = Some(error);
                    task.info.clone()
                })
            };
            if let Some(payload) = payload {
                let _ = app.emit("download-progress", payload);
            }
        }
    });
}

async fn run_download(
    app: &AppHandle,
    manager: &Manager,
    id: &str,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let (url, target, total, connections) = {
        let tasks = manager.tasks.read().await;
        let task = tasks.get(id).ok_or("找不到下载任务")?;
        (
            task.info.url.clone(),
            Path::new(&task.info.destination).join(&task.info.file_name),
            task.info.total,
            task.connections,
        )
    };
    if total == 0 {
        return Err("服务器没有提供文件大小，暂时无法进行分片下载".into());
    }
    let chunk_size = total.div_ceil(connections as u64);
    let client = http_client()?;
    let mut handles = Vec::new();
    for index in 0..connections {
        let start = index as u64 * chunk_size;
        if start >= total {
            break;
        }
        let end = ((index as u64 + 1) * chunk_size - 1).min(total - 1);
        handles.push(tokio::spawn(download_part(
            client.clone(),
            url.clone(),
            part_path(&target, index),
            start,
            end,
            cancellation.clone(),
        )));
    }

    let mut last_bytes = 0;
    let mut last_time = Instant::now();
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => return Ok(()),
            _ = tokio::time::sleep(Duration::from_millis(350)) => {
                let downloaded = count_parts(&target, handles.len()).await;
                let elapsed = last_time.elapsed().as_secs_f64();
                let speed = ((downloaded.saturating_sub(last_bytes)) as f64 / elapsed) as u64;
                last_bytes = downloaded;
                last_time = Instant::now();
                update_progress(app, manager, id, downloaded, total, speed, "downloading").await;
                if handles.iter().all(|handle| handle.is_finished()) {
                    break;
                }
            }
        }
    }
    for handle in handles {
        handle.await.map_err(|error| error.to_string())??;
    }
    merge_parts(&target, connections, total).await?;
    update_progress(app, manager, id, total, total, 0, "completed").await;
    Ok(())
}

async fn download_part(
    client: reqwest::Client,
    url: String,
    path: PathBuf,
    start: u64,
    end: u64,
    cancellation: CancellationToken,
) -> Result<(), String> {
    let existing = fs::metadata(&path)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0);
    let expected = end - start + 1;
    if existing >= expected {
        return Ok(());
    }
    let response = client
        .get(url)
        .header(RANGE, format!("bytes={}-{}", start + existing, end))
        .send()
        .await
        .map_err(friendly_error)?;
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err("服务器不支持多连接分片下载（Range）".into());
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .map_err(|error| format!("无法写入临时文件：{error}"))?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = tokio::select! {
        _ = cancellation.cancelled() => return Ok(()),
        next = stream.next() => next
    } {
        file.write_all(&chunk.map_err(friendly_error)?)
            .await
            .map_err(|error| format!("写入文件失败：{error}"))?;
    }
    Ok(())
}

async fn merge_parts(target: &Path, count: usize, total: u64) -> Result<(), String> {
    let temp_target = target.with_extension("flashget");
    let mut output = File::create(&temp_target)
        .await
        .map_err(|error| format!("无法创建目标文件：{error}"))?;
    let mut written = 0;
    for index in 0..count {
        let part = part_path(target, index);
        if !part.exists() {
            continue;
        }
        let mut input = File::open(&part).await.map_err(|error| error.to_string())?;
        let mut buffer = vec![0_u8; 1024 * 1024];
        loop {
            let size = input
                .read(&mut buffer)
                .await
                .map_err(|error| error.to_string())?;
            if size == 0 {
                break;
            }
            output
                .write_all(&buffer[..size])
                .await
                .map_err(|error| error.to_string())?;
            written += size as u64;
        }
    }
    output.flush().await.map_err(|error| error.to_string())?;
    if written != total {
        return Err(format!(
            "文件合并校验失败：期望 {total} 字节，实际 {written} 字节"
        ));
    }
    fs::rename(&temp_target, target)
        .await
        .map_err(|error| error.to_string())?;
    for index in 0..count {
        let _ = fs::remove_file(part_path(target, index)).await;
    }
    Ok(())
}

async fn count_parts(target: &Path, count: usize) -> u64 {
    let mut total = 0;
    for index in 0..count {
        total += fs::metadata(part_path(target, index))
            .await
            .map(|meta| meta.len())
            .unwrap_or(0);
    }
    total
}

pub(crate) async fn update_progress(
    app: &AppHandle,
    manager: &Manager,
    id: &str,
    downloaded: u64,
    total: u64,
    speed: u64,
    status: &str,
) {
    let payload = {
        let mut tasks = manager.tasks.write().await;
        tasks.get_mut(id).map(|task| {
            task.info.downloaded = downloaded;
            task.info.total = total;
            task.info.speed = speed;
            task.info.status = status.into();
            task.info.eta_seconds =
                estimate_eta(task.info.eta_seconds, downloaded, total, speed, status);
            task.info.clone()
        })
    };
    if let Some(payload) = payload {
        let _ = app.emit("download-progress", payload);
    }
}

fn estimate_eta(
    previous: Option<u64>,
    downloaded: u64,
    total: u64,
    speed: u64,
    status: &str,
) -> Option<u64> {
    if status != "downloading" || speed == 0 || downloaded >= total {
        return None;
    }
    let current = total.saturating_sub(downloaded).div_ceil(speed);
    Some(match previous {
        Some(previous) => ((previous as f64 * 0.7) + (current as f64 * 0.3)).round() as u64,
        None => current,
    })
}

#[cfg(test)]
mod tests {
    use super::estimate_eta;

    #[test]
    fn calculates_and_smooths_eta() {
        assert_eq!(estimate_eta(None, 500, 1_000, 100, "downloading"), Some(5));
        assert_eq!(
            estimate_eta(Some(10), 500, 1_000, 100, "downloading"),
            Some(9)
        );
        assert_eq!(estimate_eta(Some(10), 500, 1_000, 0, "downloading"), None);
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("FlashGet/0.1")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|error| error.to_string())
}

fn part_path(target: &Path, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.part.{index}", target.to_string_lossy()))
}

fn file_name_from_disposition(value: &str) -> Option<String> {
    value.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        key.eq_ignore_ascii_case("filename")
            .then(|| value.trim_matches('"').to_string())
    })
}

fn friendly_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "连接服务器超时".into()
    } else if error.is_connect() {
        "无法连接下载服务器".into()
    } else {
        error.to_string()
    }
}
