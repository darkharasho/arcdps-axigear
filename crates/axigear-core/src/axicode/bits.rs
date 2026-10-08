//! MSB-first bit reader, matching AxiCode's `BitReader`.

use super::DecodeError;

pub struct BitReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn read(&mut self, n: u32) -> Result<u32, DecodeError> {
        let mut value = 0u32;
        for _ in 0..n {
            let byte = *self.buf.get(self.pos / 8).ok_or(DecodeError::Truncated)?;
            let bit = (byte >> (7 - (self.pos % 8))) & 1;
            value = (value << 1) | bit as u32;
            self.pos += 1;
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_msb_first_across_byte_boundaries() {
        let bytes = [0b1010_0000, 0xFF];
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read(1).unwrap(), 1);
        assert_eq!(r.read(3).unwrap(), 0b010);
        assert_eq!(r.read(4).unwrap(), 0);
        assert_eq!(r.read(8).unwrap(), 255);
    }

    #[test]
    fn reading_past_the_end_is_truncated() {
        let mut r = BitReader::new(&[0xAB]);
        assert_eq!(r.read(8).unwrap(), 0xAB);
        assert_eq!(r.read(1), Err(DecodeError::Truncated));
    }

    #[test]
    fn reads_a_17_bit_id_spanning_three_bytes() {
        // 14402 = 0b0_0011_1000_0100_0010 as 17 bits, then 7 padding bits.
        let bytes = [0b0001_1100, 0b0010_0001, 0b0000_0000];
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read(17).unwrap(), 14402);
    }
}
