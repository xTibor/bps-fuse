use std::io::{Read, Result};

use byteorder::ReadBytesExt;

pub trait ReadExt: Read {
    fn read_vlq(&mut self) -> Result<u64> {
        let mut data = 0;
        let mut shift = 1;
        loop {
            let x = self.read_u8()?;
            data += ((x as u64) & 0x7F) * shift;
            if x & 0x80 != 0 {
                break;
            }
            shift <<= 7;
            data += shift;
        }
        Ok(data)
    }
}

impl<T> ReadExt for T where T: Read {}
