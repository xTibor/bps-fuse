use std::sync::{Arc, Mutex};

use notify::event::{AccessKind, AccessMode, RemoveKind};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Result, Watcher};

use crate::rom_manager::RomManager;

pub struct RomWatcher {
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
}

impl RomWatcher {
    pub fn new(rom_manager: Arc<Mutex<RomManager>>) -> Result<Self> {
        let mut watcher: RecommendedWatcher = {
            let rom_manager = rom_manager.clone();
            Watcher::new_immediate(move |result: Result<Event>| match result {
                Ok(event) => match event.kind {
                    EventKind::Access(AccessKind::Close(AccessMode::Write)) | EventKind::Remove(RemoveKind::File) => {
                        rom_manager.lock().unwrap().refresh().unwrap();
                    }
                    _ => {}
                },
                Err(e) => eprintln!("watch error: {:?}", e),
            })?
        };

        {
            let rom_manager = rom_manager.lock().unwrap();
            let base_directory = rom_manager.base_directory.as_path();
            watcher.watch(&base_directory, RecursiveMode::NonRecursive)?;
        }

        Ok(Self { watcher })
    }
}
