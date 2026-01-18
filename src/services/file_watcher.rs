//! File watcher service for dev mode.
//!
//! Monitors the screens directory for changes to Lua scripts and SVG templates,
//! broadcasting events to connected SSE clients.

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, Mutex};

/// Event sent when files change
#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    /// Paths that changed
    pub paths: Vec<PathBuf>,
}

/// File watcher that monitors the screens directory
pub struct FileWatcher {
    /// Broadcast sender for file change events
    sender: broadcast::Sender<FileChangeEvent>,
    /// Handle to the watcher (kept alive)
    _watcher: Option<RecommendedWatcher>,
    /// Flag indicating if watcher is active
    active: bool,
}

impl FileWatcher {
    /// Create a new file watcher for the given directory.
    pub fn new(watch_path: Option<PathBuf>) -> Self {
        let (sender, _) = broadcast::channel(16);

        let (watcher, active) = if let Some(path) = watch_path {
            if path.exists() {
                match Self::start_watcher(&path, sender.clone()) {
                    Ok(watcher) => {
                        tracing::info!(path = %path.display(), "File watcher started");
                        (Some(watcher), true)
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to start file watcher");
                        (None, false)
                    }
                }
            } else {
                tracing::debug!(path = %path.display(), "Watch path does not exist");
                (None, false)
            }
        } else {
            tracing::debug!("No watch path configured, file watching disabled");
            (None, false)
        };

        Self {
            sender,
            _watcher: watcher,
            active,
        }
    }

    fn start_watcher(
        path: &Path,
        sender: broadcast::Sender<FileChangeEvent>,
    ) -> Result<RecommendedWatcher, notify::Error> {
        // Create a channel for raw events
        let (tx, mut rx) = mpsc::channel::<PathBuf>(100);

        // Spawn debouncing task
        let debounce_sender = sender;
        tokio::spawn(async move {
            let pending: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));
            let pending_clone = pending.clone();

            // Debounce timer task
            let debounce_sender_clone = debounce_sender.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    let mut guard = pending_clone.lock().await;
                    if !guard.is_empty() {
                        let paths: Vec<PathBuf> = guard.drain().collect();
                        tracing::debug!(paths = ?paths, "Files changed (debounced)");
                        let _ = debounce_sender_clone.send(FileChangeEvent { paths });
                    }
                }
            });

            // Receive raw events and add to pending set
            while let Some(path) = rx.recv().await {
                pending.lock().await.insert(path);
            }
        });

        // Create watcher
        let tx_clone = tx;
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    for path in event.paths {
                        // Only watch .lua and .svg files
                        if path
                            .extension()
                            .map(|ext| ext == "lua" || ext == "svg")
                            .unwrap_or(false)
                        {
                            let _ = tx_clone.blocking_send(path);
                        }
                    }
                }
            },
            Config::default(),
        )?;

        watcher.watch(path, RecursiveMode::Recursive)?;

        Ok(watcher)
    }

    /// Subscribe to file change events
    pub fn subscribe(&self) -> broadcast::Receiver<FileChangeEvent> {
        self.sender.subscribe()
    }

    /// Check if the watcher is active
    pub fn is_active(&self) -> bool {
        self.active
    }
}

/// Shared file watcher state
pub type SharedFileWatcher = Arc<FileWatcher>;
