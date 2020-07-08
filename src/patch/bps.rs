use std::convert::TryFrom;
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use byteorder::{LittleEndian, ReadBytesExt};
use crc::crc32;
use num_enum::TryFromPrimitive;

use crate::patch::Patch;
use crate::utils::ReadExt;

const BPS_FORMAT_MARKER: [u8; 4] = [b'B', b'P', b'S', b'1'];
const BPS_FOOTER_SIZE: usize = 12;

#[derive(Debug)]
pub enum BpsError {
    OutdatedCache,
    FormatMarker,
    SourceLength,
    TargetLength,
    SourceChecksum,
    TargetChecksum,
    PatchChecksum,
}

impl fmt::Display for BpsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BpsError::OutdatedCache => write!(f, "outdated cache"),
            BpsError::FormatMarker => write!(f, "invalid format marker"),
            BpsError::SourceLength => write!(f, "source length mismatch"),
            BpsError::TargetLength => write!(f, "target length mismatch"),
            BpsError::SourceChecksum => write!(f, "invalid source checksum"),
            BpsError::TargetChecksum => write!(f, "invalid target checksum"),
            BpsError::PatchChecksum => write!(f, "invalid patch checksum"),
        }
    }
}

impl Error for BpsError {}

#[derive(Debug)]
pub struct BpsPatch {
    source_path: Option<PathBuf>,
    source_size: u64,
    source_checksum: u32,

    target_size: u64,
    target_checksum: u32,

    patch_path: PathBuf,
    patch_offset: u64,
    patch_checksum: u32,
    patch_metadata: Vec<u8>,
    patch_modified: SystemTime,
}

impl BpsPatch {
    pub fn new(patch_path: &Path) -> Result<Self, Box<dyn Error>> {
        let mut f = File::open(patch_path)?;

        let mut format_marker: [u8; 4] = [0; 4];
        f.read_exact(&mut format_marker)?;
        if format_marker != BPS_FORMAT_MARKER {
            return Err(Box::new(BpsError::FormatMarker));
        }

        let source_size = f.read_vlq()?;
        let target_size = f.read_vlq()?;
        let patch_metadata_size = f.read_vlq()?;

        let mut patch_metadata: Vec<u8> = vec![0; patch_metadata_size as usize];
        f.read_exact(&mut patch_metadata)?;

        let patch_offset = f.stream_position()?;

        f.seek(SeekFrom::End(-(BPS_FOOTER_SIZE as i64)))?;
        let source_checksum = f.read_u32::<LittleEndian>()?;
        let target_checksum = f.read_u32::<LittleEndian>()?;
        let patch_checksum = f.read_u32::<LittleEndian>()?;

        let patch_modified = f.metadata()?.modified()?;

        Ok(Self {
            source_path: None,
            source_size,
            source_checksum,
            target_size,
            target_checksum,
            patch_path: patch_path.to_owned(),
            patch_offset,
            patch_checksum,
            patch_metadata,
            patch_modified,
        })
    }

    pub fn set_source_path(&mut self, source_path: &Path) {
        self.source_path = Some(source_path.to_path_buf());
    }

    pub fn source_checksum(&self) -> u32 {
        self.source_checksum
    }
}

impl Patch for BpsPatch {
    fn target_size(&self) -> u64 {
        self.target_size
    }

    fn patched_rom(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut patch_file = File::open(&self.patch_path)?;

        if patch_file.metadata()?.modified()? != self.patch_modified {
            return Err(Box::new(BpsError::OutdatedCache));
        }

        patch_file.seek(SeekFrom::Start(self.patch_offset))?;
        let patch_end_offset = (patch_file.stream_len()? as usize) - BPS_FOOTER_SIZE;

        let source = fs::read(self.source_path.as_ref().unwrap())?;

        if source.len() as u64 != self.source_size {
            return Err(Box::new(BpsError::SourceLength));
        }

        if crc32::checksum_ieee(&source) != self.source_checksum {
            return Err(Box::new(BpsError::SourceChecksum));
        }

        let mut target = vec![0; self.target_size as usize];

        let mut output_offset = 0;
        let mut source_relative_offset = 0;
        let mut target_relative_offset = 0;

        while (patch_file.stream_position()? as usize) < patch_end_offset {
            #[derive(TryFromPrimitive)]
            #[repr(usize)]
            enum BpsCommand {
                SourceRead,
                TargetRead,
                SourceCopy,
                TargetCopy,
            }

            let (command, length) = {
                let data = patch_file.read_vlq()? as usize;
                (BpsCommand::try_from(data & 3)?, (data >> 2) + 1)
            };

            match command {
                BpsCommand::SourceRead => {
                    for i in 0..length {
                        target[output_offset + i] = source[output_offset + i];
                    }
                    output_offset += length;
                }
                BpsCommand::TargetRead => {
                    for i in 0..length {
                        target[output_offset + i] = patch_file.read_u8()?;
                    }
                    output_offset += length;
                }
                BpsCommand::SourceCopy => {
                    let offset = patch_file.read_signed_vlq()?;
                    source_relative_offset = (source_relative_offset as isize + offset as isize) as usize; // unsafe

                    for i in 0..length {
                        target[output_offset + i] = source[source_relative_offset + i];
                    }
                    output_offset += length;
                    source_relative_offset += length;
                }
                BpsCommand::TargetCopy => {
                    let offset = patch_file.read_signed_vlq()?;
                    target_relative_offset = (target_relative_offset as isize + offset as isize) as usize; // unsafe

                    for i in 0..length {
                        target[output_offset + i] = target[target_relative_offset + i];
                    }
                    output_offset += length;
                    target_relative_offset += length;
                }
            }
        }

        if target.len() as u64 != self.target_size {
            return Err(Box::new(BpsError::TargetLength));
        }

        if crc32::checksum_ieee(&target) != self.target_checksum {
            return Err(Box::new(BpsError::TargetChecksum));
        }

        Ok(target)
    }
}
