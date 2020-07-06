use std::convert::TryFrom;
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use byteorder::{LittleEndian, ReadBytesExt};
use crc::crc32;
use num_enum::TryFromPrimitive;

use crate::patch::Patch;
use crate::readext::ReadExt;

const BPS_FORMAT_MARKER: [u8; 4] = [b'B', b'P', b'S', b'1'];
const BPS_FOOTER_SIZE: usize = 12;

#[derive(Debug)]
pub enum BpsError {
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
    patch_path: PathBuf,

    source_size: u64,
    source_checksum: u32,

    target_size: u64,
    target_checksum: u32,

    patch_offset: u64,
    patch_checksum: u32,

    metadata: Vec<u8>,

    access_time: SystemTime,
    create_time: SystemTime,
    modify_time: SystemTime,
}

impl BpsPatch {
    pub fn new(patch_path: &Path) -> Result<Self, Box<dyn Error>> {
        let mut f = File::open(patch_path)?;

        // Header
        let mut format_marker: [u8; 4] = [0; 4];
        f.read_exact(&mut format_marker)?;
        if format_marker != BPS_FORMAT_MARKER {
            return Err(Box::new(BpsError::FormatMarker));
        }

        let source_size = f.read_vlq()?;
        let target_size = f.read_vlq()?;

        let metadata_size = f.read_vlq()?;
        let mut metadata: Vec<u8> = vec![0; metadata_size as usize];
        f.read_exact(&mut metadata)?;

        // Patch
        let patch_offset = f.stream_position()?;

        // Footer
        f.seek(SeekFrom::End(-(BPS_FOOTER_SIZE as i64)))?;
        let source_checksum = f.read_u32::<LittleEndian>()?;
        let target_checksum = f.read_u32::<LittleEndian>()?;
        let patch_checksum = f.read_u32::<LittleEndian>()?;

        // Patch time metadata
        let access_time = f.metadata()?.accessed()?;
        let create_time = f.metadata()?.created()?;
        let modify_time = f.metadata()?.modified()?;

        Ok(Self {
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
        let source = fs::read(self.source_path.as_ref().unwrap())?;

        if source.len() as u64 != self.source_size {
            return Err(Box::new(BpsError::SourceLength));
        }

        if crc32::checksum_ieee(&source) != self.source_checksum {
            return Err(Box::new(BpsError::SourceChecksum));
        }

        let mut target = vec![0; self.target_size as usize];

        // TODO: patch_file CRC checksum
        let mut patch_file = BufReader::new(File::open(&self.patch_path)?);
        patch_file.seek(SeekFrom::Start(self.patch_offset))?;
        let patch_end_offset = (patch_file.stream_len()? as usize) - BPS_FOOTER_SIZE;

        let mut output_offset: usize = 0;
        let mut source_relative_offset: usize = 0;
        let mut target_relative_offset: usize = 0;

        while (patch_file.stream_position()? as usize) < patch_end_offset {
            #[derive(TryFromPrimitive)]
            #[repr(u64)]
            enum BpsCommand {
                SourceRead,
                TargetRead,
                SourceCopy,
                TargetCopy,
            }

            let (command, length) = {
                let data = patch_file.read_vlq()?;
                (BpsCommand::try_from(data & 3)?, (data >> 2) + 1)
            };

            match command {
                BpsCommand::SourceRead => {
                    for _ in 0..length {
                        target[output_offset] = source[output_offset];
                        output_offset += 1;
                    }
                }
                BpsCommand::TargetRead => {
                    for _ in 0..length {
                        target[output_offset] = patch_file.read_u8()?;
                        output_offset += 1;
                    }
                }
                BpsCommand::SourceCopy => {
                    let data = patch_file.read_vlq()?;

                    let offs = (data >> 1) as usize;
                    if data & 1 != 0 {
                        source_relative_offset -= offs;
                    } else {
                        source_relative_offset += offs;
                    }

                    for _ in 0..length {
                        target[output_offset] = source[source_relative_offset];
                        output_offset += 1;
                        source_relative_offset += 1;
                    }
                }
                BpsCommand::TargetCopy => {
                    let data = patch_file.read_vlq()?;

                    let offs = (data >> 1) as usize;
                    if data & 1 != 0 {
                        target_relative_offset -= offs;
                    } else {
                        target_relative_offset += offs;
                    }

                    for _ in 0..length {
                        target[output_offset] = target[target_relative_offset];
                        output_offset += 1;
                        target_relative_offset += 1;
                    }
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
