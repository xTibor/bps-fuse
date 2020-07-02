use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crc::crc32;

use crate::bps::BpsPatch;

#[derive(Debug)]
pub struct RomManager {
    pub base_directory: PathBuf,
    pub source_roms: HashMap<u32, PathBuf>,
    pub target_roms: HashMap<PathBuf, Arc<BpsPatch>>,
}

impl RomManager {
    pub fn new(base_directory: &Path) -> Result<RomManager> {
        let mut result = Self {
            base_directory: base_directory.to_owned(),
            source_roms: HashMap::new(),
            target_roms: HashMap::new(),
        };
        result.refresh()?;
        Ok(result)
    }

    pub fn refresh(&mut self) -> Result<()> {
        eprintln!("Refreshing");
        self.source_roms.clear();
        self.target_roms.clear();

        fn is_bps_file(path: &Path) -> bool {
            match path.extension().and_then(OsStr::to_str) {
                Some("bps") | Some("BPS") => true,
                _ => false,
            }
        }

        for entry in fs::read_dir(&self.base_directory)?
            .filter_map(|e| e.ok())
            .filter(|e| !e.file_type().unwrap().is_dir())
            .filter(|e| !is_bps_file(&e.path()))
        {
            let crc = crc32::checksum_ieee(&fs::read(entry.path())?);
            self.source_roms.insert(crc, entry.path().to_owned());
        }

        for entry in fs::read_dir(&self.base_directory)?
            .filter_map(|e| e.ok())
            .filter(|e| !e.file_type().unwrap().is_dir())
            .filter(|e| is_bps_file(&e.path()))
        {
            let mut header = BpsPatch::new(&entry.path())?;

            if let Some(source_path) = self.source_roms.get(&header.source_checksum) {
                header.source_path = Some(source_path.clone());

                let mut target_path = header.patch_path.strip_prefix(&self.base_directory).unwrap().to_owned();
                target_path.set_extension(source_path.extension().unwrap_or_default());
                self.target_roms.insert(target_path, Arc::new(header));
            } else {
                eprintln!(
                    "No source ROM was found for {:?} (CRC32=0x{:08X})",
                    header.patch_path, header.source_checksum
                );
            }
        }

        // TODO: UPS support
        // With the same CRC32-matching logic as BPS

        // TODO: IPS support
        // Only when source_roms.len() == 1

        Ok(())
    }
}
