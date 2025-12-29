pub struct BitPacker<'a> {
    pub buffer: &'a mut Vec<u8>,
    pub bit_offset: u8,
}

impl<'a> BitPacker<'a> {
    pub fn new(buffer: &'a mut Vec<u8>) -> Self {
        buffer.clear();
        buffer.push(0);
        BitPacker {
            buffer,
            bit_offset: 0,
        }
    }

    pub fn write_bytes(&mut self, mut bytes: impl Iterator<Item = u8>) {
        if self.bit_offset == 0 {
            if let Some(first) = bytes.next() {
                let last = self.buffer.len() - 1;
                self.buffer[last] = first;
                self.buffer.extend(bytes);
                self.bit_offset = 8;
            }
        } else if self.bit_offset == 8 {
            self.buffer.extend(bytes);
        } else {
            for byte in bytes {
                let left_mask = byte >> self.bit_offset;
                let right_mask = byte << (8 - self.bit_offset);
                let last = self.buffer.len() - 1;
                self.buffer[last] |= left_mask;
                self.buffer.push(right_mask);
            }
        }
    }

    pub fn write_bits(&mut self, bits: u8, width: u8) {
        if width == 0 {
            return;
        }

        if self.bit_offset == 8 {
            self.buffer.push(0);
            self.bit_offset = 0;
        }

        let bits = bits & (((1u16 << width) - 1) as u8);
        let remaining = 8 - self.bit_offset;
        let last = self.buffer.len() - 1;

        if width <= remaining {
            self.buffer[last] |= bits << (remaining - width);
            self.bit_offset += width;
        } else {
            self.buffer[last] |= bits >> (width - remaining);
            let second_width = width - remaining;
            self.buffer.push(bits << (8 - second_width));
            self.bit_offset = second_width;
        }
    }

    pub fn write_bits_u32(&mut self, bits: u32, width: u8) {
        for i in (0..width).rev() {
            self.write_bit((bits >> i) & 1 != 0);
        }
    }

    pub fn write_bit(&mut self, bit: bool) {
        if self.bit_offset == 8 {
            self.buffer.push(0);
            self.bit_offset = 0;
        }

        if bit {
            let last = self.buffer.len() - 1;
            self.buffer[last] |= 1 << (7 - self.bit_offset);
        }

        self.bit_offset += 1;
    }

    pub fn write_int(&mut self, int: i64) {
        let (header, length) = if int < i8::MAX as i64 {
            (0b00, 1)
        } else if int < i16::MAX as i64 {
            (0b01, 2)
        } else if int < i32::MAX as i64 {
            (0b10, 4)
        } else {
            (0b11, 8)
        };

        self.write_bits(header, 2);
        self.write_bytes(int.to_le_bytes().into_iter().take(length));
    }

    pub fn write_strings(&mut self, strings: &[&str]) {
        use crate::huffman::{HuffmanTable, compress};

        let mut blob = Vec::new();
        for (i, s) in strings.iter().enumerate() {
            if i > 0 {
                blob.push(0);
            }
            blob.extend_from_slice(s.as_bytes());
        }

        let table = HuffmanTable::common_table();
        let compressed = compress(&blob, &table);

        self.write_int(strings.len() as i64);
        self.write_int(blob.len() as i64);
        self.write_int(compressed.len() as i64);
        self.write_bytes(compressed.into_iter());
    }

    pub fn finish(self) -> Vec<u8> {
        self.buffer.clone()
    }
}

pub struct BitUnpacker<'a> {
    pub buffer: &'a [u8],
    pub byte_index: usize,
    pub bit_offset: u8,
}

impl<'a> BitUnpacker<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        BitUnpacker {
            buffer,
            byte_index: 0,
            bit_offset: 0,
        }
    }

    pub fn read_bit(&mut self) -> bool {
        let byte = self.buffer[self.byte_index];
        let bit = (byte >> (7 - self.bit_offset)) & 1 != 0;
        self.bit_offset += 1;
        if self.bit_offset == 8 {
            self.byte_index += 1;
            self.bit_offset = 0;
        }
        bit
    }

    pub fn read_bits(&mut self, width: u8) -> u8 {
        if width == 0 {
            return 0;
        }

        let remaining = 8 - self.bit_offset;
        let byte = self.buffer[self.byte_index];

        if width <= remaining {
            let shift = remaining - width;
            let mask = ((1u16 << width) - 1) as u8;
            let result = (byte >> shift) & mask;
            self.bit_offset += width;
            if self.bit_offset == 8 {
                self.byte_index += 1;
                self.bit_offset = 0;
            }
            result
        } else {
            let first_mask = ((1u16 << remaining) - 1) as u8;
            let first_part = byte & first_mask;
            self.byte_index += 1;

            let second_width = width - remaining;
            let second_byte = self.buffer[self.byte_index];
            let second_part = second_byte >> (8 - second_width);

            self.bit_offset = second_width;
            (first_part << second_width) | second_part
        }
    }

    pub fn peek_bits(&self, width: u8) -> usize {
        let mut result = 0usize;
        let mut byte_pos = self.byte_index;
        let mut bit_pos = self.bit_offset;

        for _ in 0..width {
            if byte_pos >= self.buffer.len() {
                result <<= 1;
            } else {
                let bit = ((self.buffer[byte_pos] >> (7 - bit_pos)) & 1) as usize;
                result = (result << 1) | bit;
                bit_pos += 1;
                if bit_pos == 8 {
                    byte_pos += 1;
                    bit_pos = 0;
                }
            }
        }
        result
    }

    pub fn skip_bits(&mut self, n: u8) {
        self.bit_offset += n;
        while self.bit_offset >= 8 {
            self.byte_index += 1;
            self.bit_offset -= 8;
        }
    }

    pub fn read_bytes(&mut self, n: usize) -> Vec<u8> {
        let mut result = Vec::with_capacity(n);
        for _ in 0..n {
            result.push(self.read_byte());
        }
        result
    }

    pub fn read_byte(&mut self) -> u8 {
        if self.bit_offset == 0 {
            let byte = self.buffer[self.byte_index];
            self.byte_index += 1;
            byte
        } else {
            let remaining = 8 - self.bit_offset;
            let first_part = self.buffer[self.byte_index] & (((1u16 << remaining) - 1) as u8);
            self.byte_index += 1;
            let second_part = self.buffer[self.byte_index] >> remaining;
            (first_part << self.bit_offset) | second_part
        }
    }

    pub fn read_int(&mut self) -> i64 {
        let header = self.read_bits(2);
        let length = match header {
            0b00 => 1,
            0b01 => 2,
            0b10 => 4,
            0b11 => 8,
            _ => unreachable!(),
        };

        // 3 bit header?
        /*
        0 => 4,
        1 => 6,
        2 => 8,
        3 => 12,
        4 => 16,
        5 => 24,
        6 => 32,
        7 => 64,
        */

        let mut bytes = [0u8; 8];
        for i in 0..length {
            bytes[i] = self.read_byte();
        }
        i64::from_le_bytes(bytes)
    }

    pub fn read_strings(&mut self) -> Vec<String> {
        use crate::huffman::{HuffmanTable, decompress};

        let count = self.read_int() as usize;
        let blob_len = self.read_int() as usize;
        let compressed_len = self.read_int() as usize;
        let compressed = (0..compressed_len)
            .map(|_| self.read_byte())
            .collect::<Vec<_>>();

        let table = HuffmanTable::common_table();
        let blob = decompress(&compressed, blob_len, &table);

        if count == 0 {
            return Vec::new();
        }

        blob.split(|&b| b == 0)
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn write_bits() {
        let mut buffer = Vec::new();
        let mut packer = BitPacker::new(&mut buffer);

        packer.write_bit(true);
        packer.write_bit(true);
        packer.write_bit(true);
        packer.write_bit(true);
        assert_eq!(*packer.buffer, vec![0b11110000]);

        packer.write_bit(true);
        packer.write_bit(true);
        packer.write_bit(true);
        packer.write_bit(true);
        assert_eq!(*packer.buffer, vec![0b11111111]);

        packer.write_bit(false);
        assert_eq!(*packer.buffer, vec![0b11111111, 0b00000000]);
    }

    #[test]
    pub fn write_bytes() {
        let mut buffer = Vec::new();
        let mut packer = BitPacker::new(&mut buffer);

        packer.write_bytes([0b11111001].into_iter());
        assert_eq!(*packer.buffer, vec![0b11111001]);

        packer.write_bytes([0b00000000].into_iter());
        assert_eq!(*packer.buffer, vec![0b11111001, 0b00000000]);
    }

    #[test]
    pub fn read_bits() {
        let buffer = vec![0b11110000, 0b10101010];
        let mut unpacker = BitUnpacker::new(&buffer);

        assert_eq!(unpacker.read_bit(), true);
        assert_eq!(unpacker.read_bit(), true);
        assert_eq!(unpacker.read_bit(), true);
        assert_eq!(unpacker.read_bit(), true);
        assert_eq!(unpacker.read_bit(), false);
        assert_eq!(unpacker.read_bit(), false);
        assert_eq!(unpacker.read_bit(), false);
        assert_eq!(unpacker.read_bit(), false);

        assert_eq!(unpacker.read_bits(4), 0b1010);
        assert_eq!(unpacker.read_bits(4), 0b1010);
    }

    #[test]
    pub fn read_bytes() {
        let buffer = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let mut unpacker = BitUnpacker::new(&buffer);

        assert_eq!(unpacker.read_bytes(2), vec![0xDE, 0xAD]);
        assert_eq!(unpacker.read_bytes(2), vec![0xBE, 0xEF]);
    }

    #[test]
    pub fn roundtrip() {
        let mut buffer = Vec::new();
        let mut packer = BitPacker::new(&mut buffer);

        packer.write_bits(0b101, 3);
        packer.write_bits(0b11110000, 8);
        packer.write_bit(true);
        packer.write_bytes([0xAB, 0xCD].into_iter());

        let mut unpacker = BitUnpacker::new(&buffer);
        assert_eq!(unpacker.read_bits(3), 0b101);
        assert_eq!(unpacker.read_bits(8), 0b11110000);
        assert_eq!(unpacker.read_bit(), true);
        assert_eq!(unpacker.read_bytes(2), vec![0xAB, 0xCD]);
    }

    #[test]
    pub fn roundtrip_int() {
        let mut buffer = Vec::new();
        let mut packer = BitPacker::new(&mut buffer);

        packer.write_int(42);
        packer.write_int(1000);
        packer.write_int(100000);

        let mut unpacker = BitUnpacker::new(&buffer);
        assert_eq!(unpacker.read_int(), 42);
        assert_eq!(unpacker.read_int(), 1000);
        assert_eq!(unpacker.read_int(), 100000);
    }
}
