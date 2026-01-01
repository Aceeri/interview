use std::borrow::Cow;

use crate::{serializer::PropertyType, ultra_packer};

/// 3 bit header for the length of the integer
/// Biased towards smaller numbers, since numbers greater than 2^32 seem like they'd be fairly uncommon
/// in configurations.
const INT_WIDTHS: [u8; 8] = [4, 6, 8, 12, 16, 24, 32, 64];
const NULL_TERMINATOR: u8 = 0;

// we use 32-126 + NUL, which is 96 values out of 127 (7 bits)
pub const ASCII_MAX_VALUE: u8 = 127 - UNUSED_ASCII;
pub const UNUSED_ASCII: u8 = 1 /* DEL */ + 30 /* CONTROL CHARS - NUL */;

// Not valid rust despite being const fn :(
//const (ASCII_BUNDLE_SIZE, ASCII_BITS_PER_BUNDLE): (u8, u8) = ultra_packer::find_optimal_bundle(ASCII_MAX_VALUE as u64);
pub const ASCII_BUNDLE_SIZE: u8 = ultra_packer::find_optimal_bundle_size(ASCII_MAX_VALUE as u64);
pub const ASCII_BITS_PER_BUNDLE: u8 =
    ultra_packer::find_optimal_bits_per_bundle(ASCII_MAX_VALUE as u64);

fn compact_ascii(ascii: u8) -> u8 {
    if ascii == 0 { 0 } else { ascii - UNUSED_ASCII }
}

fn uncompact_ascii(ascii: u8) -> u8 {
    if ascii == 0 { 0 } else { ascii + UNUSED_ASCII }
}

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

    pub fn write_bytes_width(&mut self, bytes: &[u8], width: u8) {
        assert!(width > 0);

        let total_bytes = (width as usize + 7) / 8;
        let remaining_bits = width % 8;

        if remaining_bits > 0 {
            self.write_bits(bytes[total_bytes - 1], remaining_bits);
            for i in (0..total_bytes - 1).rev() {
                self.write_byte(bytes[i]);
            }
        } else {
            for i in (0..total_bytes).rev() {
                self.write_byte(bytes[i]);
            }
        }
    }

    pub fn write_byte(&mut self, byte: u8) {
        self.write_bytes(&[byte]);
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        // TODO: do we even need the 0 and 8 bit stuff? feels a bit silly
        if self.bit_offset == 0 {
            // placeholder byte exists, fill it first
            let last = self.buffer.len() - 1;
            self.buffer[last] = bytes[0];
            self.buffer.extend_from_slice(&bytes[1..]);
            self.bit_offset = 8;
        } else if self.bit_offset == 8 {
            // last byte filled, just extend
            self.buffer.extend_from_slice(bytes);
        } else {
            // disjoint, fill last and next
            for &byte in bytes {
                let left_mask = byte >> self.bit_offset;
                let right_mask = byte << (8 - self.bit_offset);
                let last = self.buffer.len() - 1;
                self.buffer[last] |= left_mask;
                self.buffer.push(right_mask);
            }
        }
    }

    pub fn write_bits(&mut self, bits: u8, width: u8) {
        assert!(width > 0);

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
        let header = INT_WIDTHS
            .iter()
            .position(|&width| int < (1i64 << width))
            .unwrap_or(7);
        let width = INT_WIDTHS[header];

        self.write_bits(header as u8, 3);
        self.write_bytes_width(&int.to_le_bytes(), width);
    }

    pub fn write_string(&mut self, string: &Cow<str>) {
        self.write_bytes(string.as_bytes());
        self.write_byte(NULL_TERMINATOR)
    }

    pub fn write_ascii_string(&mut self, string: &Cow<str>) {
        let bytes = string
            .as_bytes()
            .iter()
            .copied()
            .chain(std::iter::once(NULL_TERMINATOR));
        for byte in bytes {
            self.write_bits(byte, 7);
        }
    }

    // skipping just the msb bit saves us a bit, however we are still not using
    // 1-32 and DEL, so we have about 75% usage of the space.
    //
    // Try grouping characters to reduce this.
    pub fn write_ascii_string_ultrapacked(&mut self, string: &Cow<str>) {
        // non-pow-2 bitpacking via grouping sequences
        let mut bytes = string
            .as_bytes()
            .iter()
            .copied()
            .chain(std::iter::once(NULL_TERMINATOR));

        let mut bundle_buffer = [0u64; ASCII_BUNDLE_SIZE as usize];
        'OUTER: loop {
            // get a chunk of ASCII_BUNDLE_SIZE
            for i in 0..ASCII_BUNDLE_SIZE as usize {
                let next_byte = bytes.next();
                if i == 0 && next_byte.is_none() {
                    break 'OUTER;
                }
                bundle_buffer[i] = compact_ascii(next_byte.unwrap_or(0)) as u64;
            }

            let bundle = ultra_packer::encode(
                ASCII_BUNDLE_SIZE,
                ASCII_MAX_VALUE as u64,
                bundle_buffer.as_slice(),
            );
            ultra_packer::write_bundle(self, ASCII_BITS_PER_BUNDLE, bundle);
        }
    }

    pub fn write_property_type(&mut self, tag: PropertyType) {
        let (bits, len) = tag.to_bits();
        self.write_bits(bits, len);
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

    pub fn read_bit(&mut self) -> Option<bool> {
        let byte = self.buffer.get(self.byte_index)?;
        let bit = (byte >> (7 - self.bit_offset)) & 1 != 0;
        self.bit_offset += 1;
        if self.bit_offset == 8 {
            self.byte_index += 1;
            self.bit_offset = 0;
        }
        Some(bit)
    }

    pub fn read_bits(&mut self, width: u8) -> Option<u8> {
        assert!(width > 0);

        let remaining = 8 - self.bit_offset;
        let byte = self.buffer.get(self.byte_index)?;

        if width <= remaining {
            let shift = remaining - width;
            let mask = ((1u16 << width) - 1) as u8;
            let result = (byte >> shift) & mask;
            self.bit_offset += width;
            if self.bit_offset == 8 {
                self.byte_index += 1;
                self.bit_offset = 0;
            }
            Some(result)
        } else {
            let first_mask = ((1u16 << remaining) - 1) as u8;
            let first_part = byte & first_mask;
            self.byte_index += 1;

            let second_width = width - remaining;
            let second_byte = self.buffer.get(self.byte_index)?;
            let second_part = second_byte >> (8 - second_width);

            self.bit_offset = second_width;
            Some((first_part << second_width) | second_part)
        }
    }

    pub fn read_bytes(&mut self, n: usize) -> Option<Vec<u8>> {
        let mut result = Vec::with_capacity(n);
        for _ in 0..n {
            result.push(self.read_byte()?);
        }
        Some(result)
    }

    pub fn read_byte(&mut self) -> Option<u8> {
        if self.bit_offset == 0 {
            let byte = self.buffer.get(self.byte_index)?;
            self.byte_index += 1;
            Some(*byte)
        } else {
            let remaining = 8 - self.bit_offset;
            let first_part = self.buffer.get(self.byte_index)? & (((1u16 << remaining) - 1) as u8);
            self.byte_index += 1;
            let second_part = self.buffer.get(self.byte_index)? >> remaining;
            Some((first_part << self.bit_offset) | second_part)
        }
    }

    pub fn read_int(&mut self) -> Option<i64> {
        let header = self.read_bits(3)?;
        let width = INT_WIDTHS[header as usize];

        let mut value: u64 = 0;
        for _ in 0..width {
            value = (value << 1) | (self.read_bit()? as u64);
        }
        Some(value as i64)
    }

    pub fn read_ascii_string(&mut self) -> Option<String> {
        let mut bytes = Vec::new();
        loop {
            let byte = self.read_bits(7)?;
            if byte == 0 {
                break;
            }

            bytes.push(byte);
        }

        // from_utf8_lossy_owned would be cool
        Some(String::from_utf8_lossy(bytes.as_slice()).into_owned())
    }

    pub fn read_ascii_string_ultrapacked(&mut self) -> Option<String> {
        let mut bytes = Vec::new();

        'END: loop {
            let bundle = ultra_packer::read_bundle(self, ASCII_BITS_PER_BUNDLE)?;
            let decoded = ultra_packer::decode(ASCII_BUNDLE_SIZE, ASCII_MAX_VALUE as u64, bundle);
            for byte in decoded {
                if byte == 0 {
                    break 'END;
                }

                bytes.push(uncompact_ascii(byte as u8));
            }
        }

        // from_utf8_lossy_owned would be cool
        Some(String::from_utf8_lossy(bytes.as_slice()).into_owned())
    }

    pub fn read_string(&mut self) -> Option<String> {
        let mut bytes = Vec::new();
        loop {
            let byte = self.read_byte()?;
            if byte == 0 {
                break;
            }

            bytes.push(byte);
        }

        // from_utf8_lossy_owned would be cool
        Some(String::from_utf8_lossy(bytes.as_slice()).into_owned())
    }

    pub fn read_property_type(&mut self) -> Option<PropertyType> {
        let bits = self.read_bits(2)?;
        PropertyType::from_bits(bits)
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

        packer.write_bytes(&[0b11111001]);
        assert_eq!(*packer.buffer, vec![0b11111001]);

        packer.write_bytes(&[0b00000000]);
        assert_eq!(*packer.buffer, vec![0b11111001, 0b00000000]);
    }

    #[test]
    pub fn read_bits() {
        let buffer = vec![0b11110000, 0b10101010];
        let mut unpacker = BitUnpacker::new(&buffer);

        assert_eq!(unpacker.read_bit(), Some(true));
        assert_eq!(unpacker.read_bit(), Some(true));
        assert_eq!(unpacker.read_bit(), Some(true));
        assert_eq!(unpacker.read_bit(), Some(true));
        assert_eq!(unpacker.read_bit(), Some(false));
        assert_eq!(unpacker.read_bit(), Some(false));
        assert_eq!(unpacker.read_bit(), Some(false));
        assert_eq!(unpacker.read_bit(), Some(false));

        assert_eq!(unpacker.read_bits(4), Some(0b1010));
        assert_eq!(unpacker.read_bits(4), Some(0b1010));
    }

    #[test]
    pub fn read_bytes() {
        let buffer = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let mut unpacker = BitUnpacker::new(&buffer);

        assert_eq!(unpacker.read_bytes(2), Some(vec![0xDE, 0xAD]));
        assert_eq!(unpacker.read_bytes(2), Some(vec![0xBE, 0xEF]));
    }

    #[test]
    pub fn roundtrip() {
        let mut buffer = Vec::new();
        let mut packer = BitPacker::new(&mut buffer);

        packer.write_bits(0b101, 3);
        packer.write_bits(0b11110000, 8);
        packer.write_bit(true);
        packer.write_bytes(&[0xAB, 0xCD]);

        let mut unpacker = BitUnpacker::new(&buffer);
        assert_eq!(unpacker.read_bits(3), Some(0b101));
        assert_eq!(unpacker.read_bits(8), Some(0b11110000));
        assert_eq!(unpacker.read_bit(), Some(true));
        assert_eq!(unpacker.read_bytes(2), Some(vec![0xAB, 0xCD]));
    }

    #[test]
    pub fn roundtrip_int() {
        let mut buffer = Vec::new();
        let mut packer = BitPacker::new(&mut buffer);

        packer.write_int(42);
        packer.write_int(1000);
        packer.write_int(100000);

        let mut unpacker = BitUnpacker::new(&buffer);
        assert_eq!(unpacker.read_int(), Some(42));
        assert_eq!(unpacker.read_int(), Some(1000));
        assert_eq!(unpacker.read_int(), Some(100000));
    }
}
