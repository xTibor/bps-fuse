use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{Read, Result, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::readext::ReadExt;

#[derive(Debug)]
pub struct BpsHeader {
    pub source_path: Option<PathBuf>,
    pub patch_path: PathBuf,

    pub source_size: u64,
    pub source_checksum: u32,

    pub target_size: u64,
    pub target_checksum: u32,

    pub patch_offset: u64,
    pub patch_checksum: u32,

    pub metadata: Vec<u8>,

    pub access_time: SystemTime,
    pub create_time: SystemTime,
    pub modify_time: SystemTime,
}

impl BpsHeader {
    pub fn new(patch_path: &Path) -> Result<Self> {
        let mut f = File::open(patch_path)?;

        // Header
        let _magic = f.read_u32::<LittleEndian>()?;
        let source_size = f.read_vlq()?;
        let target_size = f.read_vlq()?;
        let metadata_size = f.read_vlq()?;
        let mut metadata: Vec<u8> = vec![0; metadata_size as usize];
        f.read_exact(&mut metadata)?;

        // Patch
        let patch_offset = f.seek(SeekFrom::Current(0))?;

        // Footer
        f.seek(SeekFrom::End(-12))?;
        let source_checksum = f.read_u32::<LittleEndian>()?;
        let target_checksum = f.read_u32::<LittleEndian>()?;
        let patch_checksum = f.read_u32::<LittleEndian>()?;

        // Patch time metadata
        let access_time = f.metadata()?.accessed()?;
        let create_time = f.metadata()?.created()?;
        let modify_time = f.metadata()?.modified()?;

        Ok(BpsHeader {
            source_path: None,
            patch_path: patch_path.to_owned(),
            source_size,
            source_checksum,
            target_size,
            target_checksum,
            patch_offset,
            patch_checksum,
            metadata,
            access_time,
            create_time,
            modify_time,
        })
    }

    pub fn generate_patched_rom(&self) -> Vec<u8> {
        // TODO
        vec![0; self.source_size as usize]
    }
}
