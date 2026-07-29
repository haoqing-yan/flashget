export type TaskStatus =
  | "queued"
  | "checking"
  | "downloading"
  | "paused"
  | "completed"
  | "failed";

export interface DownloadTask {
  id: string;
  url: string;
  fileName: string;
  destination: string;
  downloaded: number;
  total: number;
  speed: number;
  etaSeconds: number | null;
  peersConnected: number;
  peersSeen: number;
  status: TaskStatus;
  error?: string;
}

export interface ProgressPayload {
  id: string;
  downloaded: number;
  total: number;
  speed: number;
  peersConnected: number;
  peersSeen: number;
  status: TaskStatus;
  error?: string;
}

export interface TorrentFile {
  path: string;
  length: number;
}

export interface TorrentMetadata {
  name: string;
  infoHash: string;
  totalSize: number;
  pieceLength: number;
  pieceCount: number;
  trackers: string[];
  files: TorrentFile[];
  sourcePath: string;
}
