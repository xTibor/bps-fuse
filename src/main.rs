#![feature(seek_convenience)]

use std::env;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

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

    let rom_manager = RomManager::new(&PathBuf::from(&args[1])).unwrap();
    let rom_filesystem = RomFilesystem::new(rom_manager);

    let fuse_args: Vec<&OsStr> = vec![&OsStr::new("-o"), &OsStr::new("auto_unmount")];
    fuse_mt::mount(fuse_mt::FuseMT::new(rom_filesystem, 1), &args[2], &fuse_args).unwrap();
}
