use std::io::Result;
use std::path::{Path, PathBuf};

use crate::patch::Patch;

pub struct IpsPatch {
    pub source_path: PathBuf,
    pub patch_path: PathBuf,

    pub target_size: u64,
}

impl IpsPatch {
    pub fn new(patch_path: &Path, source_path: &Path) -> Result<Self> {
        Ok(IpsPatch{
            patch_path: patch_path.to_path_buf(),
            source_path: source_path.to_path_buf(),
            target_size: 0,
        })
    }
}

impl Patch for IpsPatch {
    fn target_size(&self) -> u64 {
        self.target_size
    }

    fn patched_rom(&self) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
}
