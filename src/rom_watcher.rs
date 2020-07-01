use std::sync::{Arc, Mutex};

use notify::event::{AccessKind, AccessMode, CreateKind, ModifyKind, RemoveKind, RenameMode};
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
            Watcher::new_immediate(move |result: Result<Event>| {
                //eprintln!("{:?}", result);

                match result {
                    Ok(event) => match event.kind {
                        // Delete, Modify, Rename/move out of directory
                        EventKind::Remove(RemoveKind::File)
                        | EventKind::Access(AccessKind::Close(AccessMode::Write)) // Causes dupe refreshes with the workaround.
                        | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                            rom_manager.lock().unwrap().refresh().unwrap();
                        }
                        EventKind::Create(CreateKind::File) => {
                            // The notify crate is borked on Linux, it fucks up rename events
                            // when the source is outside of the watched directory.
                            // It incorrectly reports those events as create events.
                            // According to the EventKind docs this is not the right behaviour,
                            // but I got too tired of reporting/arguing about these things.
                            // They will 100% notice things are fucked when they decide to add
                            // proper, real world test cases to their crate.
                            // Until then, just hammer it and move on.
                            {
                                use std::fs::File;
                                use std::{thread, time};

                                eprintln!("Executing workaround for notify's brain damage");
                                thread::sleep(time::Duration::from_millis(1500));

                                assert_eq!(event.paths.len(), 1);
                                let f = File::open(&event.paths[0]).unwrap();
                                if f.metadata().unwrap().len() == 0 {
                                    return;
                                }
                            }

                            rom_manager.lock().unwrap().refresh().unwrap();
                        }
                        _ => {}
                    },
                    Err(e) => eprintln!("watch error: {:?}", e),
                }
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
