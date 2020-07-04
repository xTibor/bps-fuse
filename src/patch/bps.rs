use std::fs::{self, File};
use std::io::{BufReader, Read, Result, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use byteorder::{LittleEndian, ReadBytesExt};
use crc::crc32;

use crate::patch::Patch;
use crate::readext::ReadExt;

const FOOTER_SIZE: usize = 12;

#[derive(Debug)]
pub struct BpsPatch {
    pub source_path: Option<PathBuf>,
    pub patch_path: PathBuf,

    source_size: u64,
    pub source_checksum: u32,

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
        let patch_offset = f.stream_position()?;

        // Footer
        f.seek(SeekFrom::End(-(FOOTER_SIZE as i64)))?;
        let source_checksum = f.read_u32::<LittleEndian>()?;
        let target_checksum = f.read_u32::<LittleEndian>()?;
        let patch_checksum = f.read_u32::<LittleEndian>()?;

        // Patch time metadata
        let access_time = f.metadata()?.accessed()?;
        let create_time = f.metadata()?.created()?;
        let modify_time = f.metadata()?.modified()?;

        Ok(BpsPatch {
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
}

impl Patch for BpsPatch {
    fn target_size(&self) -> u64 {
        self.target_size
    }

    fn patched_rom(&self) -> Result<Vec<u8>> {
        assert!(self.source_path.is_some());

        let source = fs::read(self.source_path.as_ref().unwrap())?;
        assert_eq!(source.len() as u64, self.source_size);
        assert_eq!(self.source_checksum, crc32::checksum_ieee(&source));

        let mut target = vec![0; self.target_size as usize];

        // TODO: patch_file CRC checksum
        let mut patch_file = BufReader::new(File::open(&self.patch_path)?);
        patch_file.seek(SeekFrom::Start(self.patch_offset))?;
        let patch_end_offset = (patch_file.stream_len()? as usize) - FOOTER_SIZE;

        let mut output_offset: usize = 0;
        let mut source_relative_offset: usize = 0;
        let mut target_relative_offset: usize = 0;

        while (patch_file.stream_position()? as usize) < patch_end_offset {
            let (command, length) = {
                let data = patch_file.read_vlq()?;
                (data & 3, (data >> 2) + 1)
            };

            match command {
                0 => {
                    // SourceRead
                    for _ in 0..length {
                        target[output_offset] = source[output_offset];
                        output_offset += 1;
                    }
                }
                1 => {
                    // TargetRead
                    for _ in 0..length {
                        target[output_offset] = patch_file.read_u8()?;
                        output_offset += 1;
                    }
                }
                2 => {
                    // SourceCopy
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
                3 => {
                    // TargetCopy
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
                _ => unreachable!(),
            }
        }

        assert_eq!(target.len() as u64, self.target_size);
        assert_eq!(self.target_checksum, crc32::checksum_ieee(&target));
        Ok(target)
    }
}
