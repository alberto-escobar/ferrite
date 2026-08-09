use std::path::Path;
use std::time::Duration;

use sqlx::SqlitePool;

use crate::scanner;

const MUSIC_DIR: &str = "./Music";
const POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Polls `./Music`'s file count and total size, and re-syncs the library
/// whenever either changes. Catches files added or deleted outside the app
/// (e.g. over SSH), which otherwise wouldn't trigger anything.
pub fn spawn(pool: SqlitePool) {
    tokio::spawn(async move {
        let mut last = fingerprint().await;

        loop {
            tokio::time::sleep(POLL_INTERVAL).await;

            let current = fingerprint().await;
            if current != last {
                println!(
                    "📂 Music folder changed ({} files, {} bytes) — syncing library",
                    current.0, current.1
                );
                scanner::sync_library(&pool).await;
                last = current;
            }
        }
    });
}

async fn fingerprint() -> (u64, u64) {
    tokio::task::spawn_blocking(|| scanner::fingerprint_dir(Path::new(MUSIC_DIR)))
        .await
        .unwrap_or((0, 0))
}
