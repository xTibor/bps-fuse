use std::io;

pub mod bps;
pub mod ips;

pub trait Patch {
    fn target_size(&self) -> u64;

    fn patched_rom(&self) -> io::Result<Vec<u8>>;
}
