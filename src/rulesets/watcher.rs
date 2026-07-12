use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use tokio::sync::mpsc;
use tracing::warn;

use crate::error::AppError;

#[derive(Debug, Clone)]
pub enum WatchEvent {
    RulesetChanged { path: PathBuf },
    CategoryMapChanged,
}

pub async fn start_watcher(
    rulesets_dir: String,
    ruleset_map_file: String,
    debounce_ms: u64,
) -> Result<mpsc::Receiver<WatchEvent>, AppError> {
    let (tx, rx) = mpsc::channel(128);

    let rulesets_dir_path = PathBuf::from(rulesets_dir);
    let ruleset_map_path = PathBuf::from(ruleset_map_file);
    let parent_for_map = ruleset_map_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut debouncer = new_debouncer(
        Duration::from_millis(debounce_ms.max(50)),
        move |result: notify_debouncer_mini::DebounceEventResult| {
            let Ok(events) = result else {
                return;
            };

            for event in events {
                let kind = event.kind;
                let path = event.path;

                let send_result = if path == ruleset_map_path {
                    tx.blocking_send(WatchEvent::CategoryMapChanged)
                } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
                    match kind {
                        DebouncedEventKind::Any | DebouncedEventKind::AnyContinuous => {
                            tx.blocking_send(WatchEvent::RulesetChanged { path })
                        }
                        _ => continue,
                    }
                } else {
                    continue;
                };

                if let Err(error) = send_result {
                    warn!(error = %error, "Failed to send watch event");
                }
            }
        },
    )
    .map_err(|error| AppError::Internal(format!("Failed to create ruleset watcher: {error}")))?;

    debouncer
        .watcher()
        .watch(&rulesets_dir_path, RecursiveMode::Recursive)
        .map_err(|error| {
            AppError::Internal(format!(
                "Failed to watch rulesets directory {}: {error}",
                rulesets_dir_path.display()
            ))
        })?;

    debouncer
        .watcher()
        .watch(&parent_for_map, RecursiveMode::NonRecursive)
        .map_err(|error| {
            AppError::Internal(format!(
                "Failed to watch ruleset map parent {}: {error}",
                parent_for_map.display()
            ))
        })?;

    // Keep watcher alive on a detached task.
    tokio::spawn(async move {
        let _keep_alive = debouncer;
        futures::future::pending::<()>().await;
    });

    Ok(rx)
}
