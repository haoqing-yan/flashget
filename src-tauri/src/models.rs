use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadTask {
    pub(crate) id: String,
    pub(crate) url: String,
    pub(crate) file_name: String,
    pub(crate) destination: String,
    pub(crate) downloaded: u64,
    pub(crate) total: u64,
    pub(crate) speed: u64,
    pub(crate) eta_seconds: Option<u64>,
    pub(crate) peers_connected: usize,
    pub(crate) peers_seen: usize,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TorrentFileInfo {
    pub(crate) path: String,
    pub(crate) length: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TorrentMetadata {
    pub(crate) name: String,
    pub(crate) info_hash: String,
    pub(crate) total_size: u64,
    pub(crate) piece_length: u64,
    pub(crate) piece_count: usize,
    pub(crate) trackers: Vec<String>,
    pub(crate) files: Vec<TorrentFileInfo>,
    pub(crate) source_path: String,
}
