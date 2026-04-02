use crate::clipboard;
use crate::db::Store;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const MAX_ENTRY_SIZE: usize = 1_000_000;

pub struct Watcher {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Watcher {
    pub fn spawn(db_path: &Path) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();
        let path = db_path.to_path_buf();

        let handle = thread::spawn(move || {
            poll_loop(&path, &r, clipboard::get_clipboard);
        });

        Watcher {
            running,
            handle: Some(handle),
        }
    }

    pub fn stop(mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            h.join().ok();
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

fn should_store(content: &Option<String>) -> bool {
    content.as_ref().is_some_and(|t| t.len() <= MAX_ENTRY_SIZE)
}

fn poll_loop<F>(db_path: &Path, running: &AtomicBool, read_clipboard: F)
where
    F: Fn() -> (Option<String>, i64),
{
    let store = match Store::open(db_path) {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut last_change_count: i64 = -1;

    while running.load(Ordering::Relaxed) {
        let (content, change_count) = read_clipboard();

        if change_count != last_change_count {
            last_change_count = change_count;
            if should_store(&content) {
                store.insert(content.as_deref().unwrap(), None).ok();
            }
        }

        thread::sleep(POLL_INTERVAL);
    }
}
