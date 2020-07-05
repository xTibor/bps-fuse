use std::io::Result;
use std::sync::{Arc, Mutex};
use std::thread;

use inotify::{EventMask, Inotify, WatchMask};

use crate::rom_manager::RomManager;

pub struct RomWatcher {
    #[allow(dead_code)]
    inotify: Arc<Mutex<Inotify>>,
}

impl RomWatcher {
    pub fn new(rom_manager: Arc<Mutex<RomManager>>) -> Result<Self> {
        let inotify = Arc::new(Mutex::new(Inotify::init()?));

        {
            let rom_manager = rom_manager.lock().unwrap();
            let base_directory = rom_manager.base_directory.as_path();
            inotify
                .lock()
                .unwrap()
                .add_watch(base_directory, WatchMask::ALL_EVENTS)?;
        }

        {
            let inotify = inotify.clone();
            thread::spawn(move || {
                let mut buffer = [0u8; 4096];
                loop {
                    let events = inotify.lock().unwrap().read_events_blocking(&mut buffer).unwrap();

                    let mut changed = false;

                    for event in events {
                        changed |= event.mask.contains(EventMask::MOVED_FROM);
                        changed |= event.mask.contains(EventMask::MOVED_TO);
                        changed |= event.mask.contains(EventMask::DELETE);
                        changed |= event.mask.contains(EventMask::CLOSE_WRITE);
                    }

                    if changed {
                        if let Err(err) = rom_manager.lock().unwrap().refresh() {
                            eprintln!("Failed to refresh ROMs: {}", err);
                        }
                    }
                }
            });
        }

        Ok(Self { inotify })
    }
}
