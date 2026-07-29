mod http_download;
mod models;
mod state;
mod torrent;

use http_download::{create_download, list_tasks, pause_download, resume_download};
use state::DownloadManager;
use std::sync::Arc;
use torrent::{create_torrent_download, parse_torrent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(DownloadManager::default()))
        .invoke_handler(tauri::generate_handler![
            list_tasks,
            parse_torrent,
            create_torrent_download,
            create_download,
            pause_download,
            resume_download
        ])
        .run(tauri::generate_context!())
        .expect("error while running FlashGet");
}
