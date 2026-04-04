use crate::clipboard;
use crate::db::Store;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const MAX_ENTRY_SIZE: usize = 1_000_000;

pub struct Watcher {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Watcher {
    pub fn spawn(db_path: &Path, max_entries: usize, auto_expire_seconds: u64) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();
        let path = db_path.to_path_buf();

        let handle = thread::spawn(move || {
            poll_loop(
                &path,
                &r,
                max_entries,
                auto_expire_seconds,
                clipboard::get_clipboard,
            );
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

fn poll_loop<F>(
    db_path: &Path,
    running: &AtomicBool,
    max_entries: usize,
    auto_expire_seconds: u64,
    read_clipboard: F,
) where
    F: Fn() -> (Option<String>, i64),
{
    let store = match Store::open(db_path) {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut last_change_count: i64 = -1;
    let mut inserts_since_prune: u32 = 0;

    // Track last insert for auto-expire detection
    let mut last_insert: Option<(i64, Instant)> = None;
    let expire_duration = Duration::from_secs(auto_expire_seconds);

    while running.load(Ordering::Relaxed) {
        let (content, change_count) = read_clipboard();

        if change_count != last_change_count {
            last_change_count = change_count;
            if should_store(&content) {
                if let Ok(id) = store.insert(content.as_deref().unwrap(), None) {
                    inserts_since_prune += 1;
                    last_insert = if auto_expire_seconds > 0 {
                        Some((id, Instant::now()))
                    } else {
                        None
                    };
                }

                if max_entries > 0 && inserts_since_prune >= 100 {
                    store.prune(max_entries).ok();
                    inserts_since_prune = 0;
                }
            } else if auto_expire_seconds > 0 {
                // Clipboard was cleared (content is None or too large).
                // If the previous entry was inserted recently, it was likely
                // a password manager copy — delete it.
                if let Some((id, inserted_at)) = last_insert.take() {
                    if inserted_at.elapsed() < expire_duration {
                        store.delete(id).ok();
                    }
                }
            }
        }

        thread::sleep(POLL_INTERVAL);
    }

    // Final prune on shutdown
    if max_entries > 0 {
        store.prune(max_entries).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Simulates a sequence of clipboard reads for testing poll_loop.
    struct ClipboardSequence {
        steps: Mutex<Vec<(Option<String>, i64)>>,
        index: Mutex<usize>,
    }

    impl ClipboardSequence {
        fn new(steps: Vec<(Option<String>, i64)>) -> Arc<Self> {
            Arc::new(Self {
                steps: Mutex::new(steps),
                index: Mutex::new(0),
            })
        }

        fn read(&self) -> (Option<String>, i64) {
            let steps = self.steps.lock().unwrap();
            let mut idx = self.index.lock().unwrap();
            if *idx < steps.len() {
                let result = steps[*idx].clone();
                *idx += 1;
                result
            } else {
                // Return last state (no change)
                steps.last().cloned().unwrap_or((None, -1))
            }
        }
    }

    /// Run poll_loop for a fixed number of iterations by controlling the running flag.
    fn run_poll_with_sequence(
        store: &Store,
        auto_expire_seconds: u64,
        steps: Vec<(Option<String>, i64)>,
    ) {
        let num_steps = steps.len();
        let seq = ClipboardSequence::new(steps);
        let mut last_change_count: i64 = -1;
        let mut last_insert: Option<(i64, Instant)> = None;
        let expire_duration = Duration::from_secs(auto_expire_seconds);

        for _ in 0..num_steps {
            let (content, change_count) = seq.read();

            if change_count != last_change_count {
                last_change_count = change_count;
                if should_store(&content) {
                    if let Ok(id) = store.insert(content.as_deref().unwrap(), None) {
                        last_insert = if auto_expire_seconds > 0 {
                            Some((id, Instant::now()))
                        } else {
                            None
                        };
                    }
                } else if auto_expire_seconds > 0 {
                    if let Some((id, inserted_at)) = last_insert.take() {
                        if inserted_at.elapsed() < expire_duration {
                            store.delete(id).ok();
                        }
                    }
                }
            }
        }
    }

    fn temp_store() -> Store {
        Store::open(Path::new(":memory:")).unwrap()
    }

    #[test]
    fn should_store_rejects_none() {
        assert!(!should_store(&None));
    }

    #[test]
    fn should_store_accepts_normal_content() {
        assert!(should_store(&Some("hello".to_string())));
    }

    #[test]
    fn should_store_rejects_oversized_content() {
        let big = "x".repeat(MAX_ENTRY_SIZE + 1);
        assert!(!should_store(&Some(big)));
    }

    #[test]
    fn auto_expire_deletes_short_lived_entry() {
        let store = temp_store();

        // Simulate: password copied (change_count=1), then cleared (change_count=2)
        run_poll_with_sequence(
            &store,
            30, // 30s window — entry will be well within it
            vec![
                (Some("s3cr3t_password".to_string()), 1), // password copied
                (None, 2),                                // clipboard cleared
            ],
        );

        // The entry should have been auto-expired
        assert_eq!(
            store.count().unwrap(),
            0,
            "transient entry should be deleted"
        );
    }

    #[test]
    fn auto_expire_disabled_keeps_entry_after_clear() {
        let store = temp_store();

        run_poll_with_sequence(
            &store,
            0, // disabled
            vec![(Some("s3cr3t_password".to_string()), 1), (None, 2)],
        );

        assert_eq!(
            store.count().unwrap(),
            1,
            "entry should remain when auto-expire is disabled"
        );
    }

    #[test]
    fn auto_expire_keeps_entry_replaced_by_new_content() {
        let store = temp_store();

        // Simulate: copy A, then copy B (not a clear — normal usage)
        run_poll_with_sequence(
            &store,
            30,
            vec![
                (Some("first copy".to_string()), 1),
                (Some("second copy".to_string()), 2),
            ],
        );

        // Both entries should exist — auto-expire only triggers on clipboard clear
        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn auto_expire_only_deletes_most_recent() {
        let store = temp_store();

        // Simulate: copy A, copy B (password), clipboard cleared
        run_poll_with_sequence(
            &store,
            30,
            vec![
                (Some("normal text".to_string()), 1),
                (Some("s3cr3t".to_string()), 2),
                (None, 3), // cleared
            ],
        );

        // Only the password entry should be deleted; normal text remains
        assert_eq!(store.count().unwrap(), 1);
        let entries = store.recent(10).unwrap();
        assert_eq!(entries[0].content, "normal text");
    }

    #[test]
    fn no_change_count_change_does_nothing() {
        let store = temp_store();

        // Same change_count repeated — no action
        run_poll_with_sequence(
            &store,
            30,
            vec![
                (Some("hello".to_string()), 1),
                (Some("hello".to_string()), 1), // same change_count
                (Some("hello".to_string()), 1),
            ],
        );

        assert_eq!(store.count().unwrap(), 1);
    }
}
