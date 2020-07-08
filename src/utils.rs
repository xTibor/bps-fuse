use std::io::{self, Read};

use byteorder::ReadBytesExt;

pub trait ReadExt: Read {
    fn read_vlq(&mut self) -> io::Result<u64> {
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

    fn read_signed_vlq(&mut self) -> io::Result<i64> {
        let data = self.read_vlq()?;

        let value = (data >> 1) as i64;
        if data & 1 != 0 {
            Ok(-value)
        } else {
            Ok(value)
        }
    }
}

impl<T> ReadExt for T where T: Read {}
