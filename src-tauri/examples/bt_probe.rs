use librqbit::{AddTorrent, AddTorrentOptions, PeerConnectionOptions, Session, SessionOptions};
use librqbit_dht::PersistentDhtConfig;
use std::{path::PathBuf, time::Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let torrent = std::env::args().nth(1).expect("torrent path");
    let session = Session::new_with_opts(
        PathBuf::from("/tmp/flashget-bt-probe"),
        SessionOptions {
            disable_dht_persistence: false,
            dht_config: Some(PersistentDhtConfig {
                dump_interval: Some(Duration::from_secs(5)),
                config_filename: Some(PathBuf::from("/tmp/flashget-bt-probe-dht.json")),
            }),
            listen_port_range: Some(49152..65535),
            enable_upnp_port_forwarding: true,
            fastresume: true,
            defer_writes_up_to: Some(256),
            concurrent_init_limit: Some(8),
            peer_opts: Some(PeerConnectionOptions {
                connect_timeout: Some(Duration::from_secs(8)),
                read_write_timeout: Some(Duration::from_secs(30)),
                keep_alive_interval: Some(Duration::from_secs(60)),
            }),
            ..Default::default()
        },
    )
    .await?;
    let bytes = tokio::fs::read(torrent).await?;
    let handle = session
        .add_torrent(
            AddTorrent::from_bytes(bytes),
            Some(AddTorrentOptions {
                overwrite: true,
                ..Default::default()
            }),
        )
        .await?
        .into_handle()
        .expect("torrent handle");
    for _ in 0..12 {
        let stats = handle.stats();
        println!("{stats:?}");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    session.stop().await;
    Ok(())
}
