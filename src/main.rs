#![feature(seek_convenience)]
#![feature(slice_fill)]

use std::env;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process;
use std::sync::{Arc, Mutex};

mod patch;
mod rom_filesystem;
mod rom_manager;
mod rom_watcher;
mod utils;

use rom_filesystem::RomFilesystem;
use rom_manager::RomManager;
use rom_watcher::RomWatcher;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<OsString> = env::args_os().collect();

    if args.len() != 3 {
        println!("Usage: {} <base_directory> <mount_point>", &env::args().next().unwrap());
        process::exit(-1);
    }

    pretty_env_logger::init();

    let base_directory = PathBuf::from(&args[1]);
    let rom_manager = Arc::new(Mutex::new(RomManager::new(&base_directory)?));

    let rom_filesystem = RomFilesystem::new(rom_manager.clone());
    let _rom_watcher = RomWatcher::new(rom_manager.clone())?;

    let fuse_args: Vec<&OsStr> = vec![&OsStr::new("-o"), &OsStr::new("auto_unmount")];
    fuse_mt::mount(fuse_mt::FuseMT::new(rom_filesystem, 1), &args[2], &fuse_args)?;

    Ok(())
}
