use crate::models::DownloadTask;
use librqbit::{ManagedTorrent, Session};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64},
        Arc,
    },
};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

pub(crate) struct ManagedTask {
    pub(crate) info: DownloadTask,
    pub(crate) connections: usize,
    pub(crate) cancellation: CancellationToken,
}

#[derive(Default)]
pub(crate) struct DownloadManager {
    pub(crate) tasks: RwLock<HashMap<String, ManagedTask>>,
    pub(crate) bt: RwLock<HashMap<String, Arc<ManagedTorrent>>>,
    pub(crate) bt_session: Mutex<Option<Arc<Session>>>,
    pub(crate) shutdown_when_done: AtomicBool,
    pub(crate) shutdown_scheduled: AtomicBool,
    pub(crate) shutdown_generation: AtomicU64,
}

pub(crate) type Manager = Arc<DownloadManager>;
