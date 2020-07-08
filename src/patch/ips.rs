use std::cmp;
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use byteorder::{BigEndian, ReadBytesExt};

use crate::patch::Patch;

const IPS_FORMAT_MARKER: [u8; 5] = [b'P', b'A', b'T', b'C', b'H'];
const IPS_EOF_MARKER: usize = 0x454F46;

#[derive(Debug)]
pub enum IpsError {
    FormatMarker,
}

impl fmt::Display for IpsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            IpsError::FormatMarker => write!(f, "invalid format marker"),
        }
    }
}

impl Error for IpsError {}

pub struct IpsPatch {
    source_path: PathBuf,
    patch_path: PathBuf,

    target_size: u64,
    truncated_size: Option<u64>,
}

impl IpsPatch {
    pub fn new(patch_path: &Path, source_path: &Path) -> Result<Self, Box<dyn Error>> {
        let mut patch_file = File::open(patch_path)?;

        let mut target_size: u64 = {
            let source_file = File::open(source_path)?;
            source_file.metadata()?.len()
        };

        let mut format_marker: [u8; 5] = [0; 5];
        patch_file.read_exact(&mut format_marker)?;
        if format_marker != IPS_FORMAT_MARKER {
            return Err(Box::new(IpsError::FormatMarker));
        }

        loop {
            let offset = patch_file.read_u24::<BigEndian>()? as usize;
            if offset == IPS_EOF_MARKER {
                break;
            }

            let size = patch_file.read_u16::<BigEndian>()? as usize;
            if size == 0 {
                let rle_size = patch_file.read_u16::<BigEndian>()? as usize;
                let _rle_value = patch_file.read_u8()?;
                target_size = cmp::max(target_size, offset as u64 + rle_size as u64);
            } else {
                patch_file.seek(SeekFrom::Current(size as i64))?;
                target_size = cmp::max(target_size, offset as u64 + size as u64);
            }
        }

        let truncated_size = patch_file.read_u24::<BigEndian>().ok().map(u64::from);

        Ok(Self {
            patch_path: patch_path.to_path_buf(),
            source_path: source_path.to_path_buf(),
            target_size,
            truncated_size,
        })
    }
}

impl Patch for IpsPatch {
    fn target_size(&self) -> u64 {
        self.truncated_size.unwrap_or(self.target_size)
    }

    fn patched_rom(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut target = fs::read(&self.source_path)?;
        target.resize(self.target_size as usize, 0);

        let mut patch_file = File::open(&self.patch_path)?;

        let mut format_marker: [u8; 5] = [0; 5];
        patch_file.read_exact(&mut format_marker)?;
        if format_marker != IPS_FORMAT_MARKER {
            return Err(Box::new(IpsError::FormatMarker));
        }

        loop {
            let offset = patch_file.read_u24::<BigEndian>()? as usize;
            if offset == IPS_EOF_MARKER {
                break;
            }

            let size = patch_file.read_u16::<BigEndian>()? as usize;
            if size == 0 {
                let rle_size = patch_file.read_u16::<BigEndian>()? as usize;
                let rle_value = patch_file.read_u8()?;
                target[offset..(offset + rle_size)].fill(rle_value);
            } else {
                patch_file.read_exact(&mut target[offset..(offset + size)])?;
            }
        }

        if let Some(truncated_size) = self.truncated_size {
            target.resize(truncated_size as usize, 0);
        }

        Ok(target)
    }
}
