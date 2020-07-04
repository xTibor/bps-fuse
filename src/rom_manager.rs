use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, DirEntry};
use std::io::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crc::crc32;

use crate::patch::Patch;
use crate::patch::bps::BpsPatch;
use crate::patch::ips::IpsPatch;

#[rustfmt::skip]
const ROM_EXTENSIONS: &[&str] = &[
    // Generic
    "bin", "rom", "crt",
    // Nintendo
    "nes", "fds",        // Famicom / NES
    "sfc", "smc",        // Super Famicom / SNES
    "vb",                // Virtual Boy
    "n64", "v64", "z64", // Nintendo 64
    "gb",                // Game Boy
    "gbc",               // Game Boy Color
    "gba", "agb",        // Game Boy Advance
    "nds",               // Nintendo DS
    "3ds",               // Nintendo 3DS
];

pub struct RomManager {
    pub base_directory: PathBuf,
    pub source_roms: HashMap<u32, PathBuf>,
    pub target_roms: HashMap<PathBuf, Arc<dyn Patch + Send + Sync>>,
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

        fn extension_matches(path: &Path, extensions: &[&str]) -> bool {
            let extension = path
                .extension()
                .and_then(OsStr::to_str)
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            extensions.contains(&extension.as_str())
        }

        let entries: Vec<DirEntry> = fs::read_dir(&self.base_directory)?
            .filter_map(|e| e.ok())
            .filter(|e| !e.file_type().unwrap().is_dir())
            .collect();

        for entry in entries.iter().filter(|e| extension_matches(&e.path(), ROM_EXTENSIONS)) {
            let crc = crc32::checksum_ieee(&fs::read(entry.path())?);
            self.source_roms.insert(crc, entry.path().to_owned());
        }

        if self.source_roms.is_empty() {
            eprintln!("No source ROMs were found in {:?}", self.base_directory);
            return Ok(())
        }

        for entry in entries.iter().filter(|e| extension_matches(&e.path(), &["bps"])) {
            let mut patch = BpsPatch::new(&entry.path())?;

            if let Some(source_path) = self.source_roms.get(&patch.source_checksum) {
                patch.source_path = Some(source_path.clone());

                let mut target_path = patch.patch_path.strip_prefix(&self.base_directory).unwrap().to_owned();
                target_path.set_extension(source_path.extension().unwrap_or_default());
                self.target_roms.insert(target_path, Arc::new(patch));
            } else {
                eprintln!(
                    "No source ROM was found for {:?} (CRC32=0x{:08X})",
                    patch.patch_path, patch.source_checksum
                );
            }
        }

        for entry in entries.iter().filter(|e| extension_matches(&e.path(), &["ips"])) {
            if self.source_roms.len() > 1 {
                eprintln!("Multiple source ROMs were found for {:?}, cannot decide which one to choose", entry.path());
            } else {
                let source_path = self.source_roms.values().next().unwrap();
                let patch = IpsPatch::new(&entry.path(), source_path)?;

                let mut target_path = patch.patch_path.strip_prefix(&self.base_directory).unwrap().to_owned();
                target_path.set_extension(source_path.extension().unwrap_or_default());
                self.target_roms.insert(target_path, Arc::new(patch));
            }
        }

        // TODO: UPS support
        // With the same CRC32-matching logic as BPS

        // TODO: IPS support
        // Only when source_roms.len() == 1

        Ok(())
    }
}
