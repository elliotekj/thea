use crate::{rebuild_site, CONFIG};
use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

pub fn watch_files() {
    thread::spawn(move || {
        let (tx, rx) = channel();
        let mut watcher = watcher(tx, Duration::from_secs(1)).unwrap();
        let content_path = CONFIG.get_str("content.path").unwrap();
        let templates_path = CONFIG.get_str("templates.path").unwrap();

        let _ = watcher
            .watch(content_path, RecursiveMode::Recursive)
            .map_err(|e| error!("Watcher error: {}", e));

        let _ = watcher
            .watch(templates_path, RecursiveMode::Recursive)
            .map_err(|e| error!("Watcher error: {}", e));

        loop {
            match rx.recv() {
                Ok(event) => match event {
                    DebouncedEvent::Create(_)
                    | DebouncedEvent::Write(_)
                    | DebouncedEvent::Rename(_, _)
                    | DebouncedEvent::Remove(_) => {
                        info!("File system event occurred; regenerating the HashMap.");
                        rebuild_site();
                    }
                    _ => {}
                },
                Err(e) => error!("Watch error: {:?}", e),
            }
        }
    });
}
