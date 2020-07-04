use std::io::Result;

pub mod bps;
pub mod ips;

pub trait Patch {
    fn target_size(&self) -> u64;

    fn patched_rom(&self) -> Result<Vec<u8>>;
}
