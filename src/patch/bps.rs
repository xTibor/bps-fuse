use std::convert::TryFrom;
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{Cursor, Read, Seek, SeekFrom};
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
    FormatMarker { expected: [u8; 4], received: [u8; 4] },
    SourceLength { expected: u64, received: u64 },
    TargetLength { expected: u64, received: u64 },
    SourceChecksum { expected: u32, received: u32 },
    TargetChecksum { expected: u32, received: u32 },
    PatchChecksum { expected: u32, received: u32 },
}

impl fmt::Display for BpsError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BpsError::OutdatedCache => write!(formatter, "outdated cache"),
            BpsError::FormatMarker { expected, received } => write!(
                formatter,
                "invalid format marker (expected: {:?}, received: {:?})",
                expected, received
            ),
            BpsError::SourceLength { expected, received } => write!(
                formatter,
                "source length mismatch (expected: {}, received: {})",
                expected, received
            ),
            BpsError::TargetLength { expected, received } => write!(
                formatter,
                "target length mismatch (expected: {}, received: {})",
                expected, received
            ),
            BpsError::SourceChecksum { expected, received } => write!(
                formatter,
                "invalid source checksum (expected: 0x{:08X}, received: 0x{:08X})",
                expected, received
            ),
            BpsError::TargetChecksum { expected, received } => write!(
                formatter,
                "invalid target checksum (expected: 0x{:08X}, received: 0x{:08X})",
                expected, received
            ),
            BpsError::PatchChecksum { expected, received } => write!(
                formatter,
                "invalid patch checksum (expected: 0x{:08X}, received: 0x{:08X})",
                expected, received
            ),
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
        let mut patch_file = File::open(patch_path)?;

        let mut format_marker: [u8; 4] = [0; 4];
        patch_file.read_exact(&mut format_marker)?;
        if format_marker != BPS_FORMAT_MARKER {
            return Err(Box::new(BpsError::FormatMarker {
                expected: BPS_FORMAT_MARKER,
                received: format_marker,
            }));
        }

        let source_size = patch_file.read_vlq()?;
        let target_size = patch_file.read_vlq()?;
        let patch_metadata_size = patch_file.read_vlq()?;

        let mut patch_metadata: Vec<u8> = vec![0; patch_metadata_size as usize];
        patch_file.read_exact(&mut patch_metadata)?;

        let patch_offset = patch_file.stream_position()?;

        patch_file.seek(SeekFrom::End(-(BPS_FOOTER_SIZE as i64)))?;
        let source_checksum = patch_file.read_u32::<LittleEndian>()?;
        let target_checksum = patch_file.read_u32::<LittleEndian>()?;
        let patch_checksum = patch_file.read_u32::<LittleEndian>()?;

        let patch_modified = patch_file.metadata()?.modified()?;

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
        let patch_data = {
            let mut patch_file = File::open(&self.patch_path)?;

            if patch_file.metadata()?.modified()? != self.patch_modified {
                return Err(Box::new(BpsError::OutdatedCache));
            }

            let mut patch_data = Vec::new();
            patch_file.read_to_end(&mut patch_data)?;
            patch_data
        };

        let patch_checksum = crc32::checksum_ieee(&patch_data[0..(patch_data.len() - 4)]);
        if patch_checksum != self.patch_checksum {
            return Err(Box::new(BpsError::PatchChecksum {
                expected: self.patch_checksum,
                received: patch_checksum,
            }));
        }

        let mut patch_cursor =
            Cursor::new(&patch_data[self.patch_offset as usize..(patch_data.len() - BPS_FOOTER_SIZE)]);

        let source = fs::read(self.source_path.as_ref().unwrap())?;

        if source.len() as u64 != self.source_size {
            return Err(Box::new(BpsError::SourceLength {
                expected: self.source_size,
                received: source.len() as u64,
            }));
        }

        let source_checksum = crc32::checksum_ieee(&source);
        if source_checksum != self.source_checksum {
            return Err(Box::new(BpsError::SourceChecksum {
                expected: self.source_checksum,
                received: source_checksum,
            }));
        }

        let mut target = vec![0; self.target_size as usize];

        let mut output_offset = 0;
        let mut source_relative_offset = 0;
        let mut target_relative_offset = 0;

        while patch_cursor.stream_position()? < patch_cursor.stream_len()? {
            #[derive(TryFromPrimitive)]
            #[repr(usize)]
            enum BpsCommand {
                SourceRead,
                TargetRead,
                SourceCopy,
                TargetCopy,
            }

            let (command, length) = {
                let data = patch_cursor.read_vlq()? as usize;
                (BpsCommand::try_from(data & 3)?, (data >> 2) + 1)
            };

            match command {
                BpsCommand::SourceRead => {
                    target[output_offset..(output_offset + length)]
                        .clone_from_slice(&source[output_offset..(output_offset + length)]);
                    output_offset += length;
                }
                BpsCommand::TargetRead => {
                    patch_cursor.read_exact(&mut target[output_offset..(output_offset + length)])?;
                    output_offset += length;
                }
                BpsCommand::SourceCopy => {
                    let offset = patch_cursor.read_signed_vlq()?;
                    source_relative_offset = (source_relative_offset as isize + offset as isize) as usize; // unsafe

                    target[output_offset..(output_offset + length)]
                        .clone_from_slice(&source[source_relative_offset..(source_relative_offset + length)]);

                    source_relative_offset += length;
                    output_offset += length;
                }
                BpsCommand::TargetCopy => {
                    let offset = patch_cursor.read_signed_vlq()?;
                    target_relative_offset = (target_relative_offset as isize + offset as isize) as usize; // unsafe

                    for i in 0..length {
                        target[output_offset + i] = target[target_relative_offset + i];
                    }

                    target_relative_offset += length;
                    output_offset += length;
                }
            }
        }

        if target.len() as u64 != self.target_size {
            return Err(Box::new(BpsError::TargetLength {
                expected: self.target_size,
                received: target.len() as u64,
            }));
        }

        let target_checksum = crc32::checksum_ieee(&target);
        if target_checksum != self.target_checksum {
            return Err(Box::new(BpsError::TargetChecksum {
                expected: self.target_checksum,
                received: target_checksum,
            }));
        }

        Ok(target)
    }
}
