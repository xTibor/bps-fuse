#![feature(seek_convenience)]

use std::env;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use notify::event::{AccessKind, AccessMode, RemoveKind};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

mod bps;
mod readext;
mod rom_filesystem;
mod rom_manager;

use rom_filesystem::RomFilesystem;
use rom_manager::RomManager;

fn main() {
    let args: Vec<OsString> = env::args_os().collect();

    if args.len() != 3 {
        println!("Usage: {} <base_directory> <mountpoint>", &env::args().next().unwrap());
        ::std::process::exit(-1);
    }

    let base_directory = PathBuf::from(&args[1]);
    let rom_manager = Arc::new(Mutex::new(RomManager::new(&base_directory).unwrap()));
    let rom_filesystem = RomFilesystem::new(rom_manager.clone());

    // TODO: Extract into a module
    let mut watcher: RecommendedWatcher = Watcher::new_immediate(move |res: notify::Result<notify::Event>| match res {
        Ok(event) => match event.kind {
            EventKind::Access(AccessKind::Close(AccessMode::Write)) | EventKind::Remove(RemoveKind::File) => {
                rom_manager.clone().lock().unwrap().refresh().unwrap();
            }
            _ => {}
        },
        Err(e) => eprintln!("watch error: {:?}", e),
    })
    .unwrap();

    watcher.watch(&base_directory, RecursiveMode::NonRecursive).unwrap();

    let fuse_args: Vec<&OsStr> = vec![&OsStr::new("-o"), &OsStr::new("auto_unmount")];
    fuse_mt::mount(fuse_mt::FuseMT::new(rom_filesystem, 1), &args[2], &fuse_args).unwrap();
}
