use std::borrow::Cow;

use crate::{serializer::PropertyType, ultra_packer};

/// 3 bit header for the length of the integer
/// Biased towards smaller numbers, since numbers greater than 2^32 seem like they'd be fairly uncommon
/// in configurations.
///
/// Maybe we could steal some ideas from utf-8?
// const INT_WIDTHS: [u8; 8] = [4, 6, 8, 12, 16, 24, 32, 64];
const INT_WIDTHS: [u8; 8] = [4, 6, 9, 13, 15, 24, 45, 64];
const NULL_TERMINATOR: u8 = 0;

// we use 32-126 + NUL, which is 96 values out of 127 (7 bits)
pub const ASCII_MAX_VALUE: u8 = 127 - UNUSED_ASCII;
pub const UNUSED_ASCII: u8 = 1 /* DEL */ + 30 /* CONTROL CHARS - NUL */;

// Not valid rust :(
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

    fn ensure_space(&mut self) {
        if self.bit_offset == 8 {
            self.buffer.push(0);
            self.bit_offset = 0;
        }
    }

    pub fn write_bit(&mut self, bit: bool) {
        self.ensure_space();
        let last = self.buffer.len() - 1;
        self.buffer[last] |= (bit as u8) << (7 - self.bit_offset);
        self.bit_offset += 1;
    }

    pub fn write_bits(&mut self, bits: u8, width: u8) {
        self.ensure_space();
        let bits = bits & ((1u16 << width) - 1) as u8;
        let space = 8 - self.bit_offset;
        let last = self.buffer.len() - 1;

        if width <= space {
            self.buffer[last] |= bits << (space - width);
            self.bit_offset += width;
        } else {
            let overflow = width - space;
            self.buffer[last] |= bits >> overflow;
            self.buffer.push(bits << (8 - overflow));
            self.bit_offset = overflow;
        }
    }

    pub fn write_byte(&mut self, byte: u8) {
        self.ensure_space();
        let last = self.buffer.len() - 1;

        if self.bit_offset == 0 {
            self.buffer[last] = byte;
            self.bit_offset = 8;
        } else {
            self.buffer[last] |= byte >> self.bit_offset;
            self.buffer.push(byte << (8 - self.bit_offset));
        }
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_byte(byte);
        }
    }

    pub fn write_bytes_width(&mut self, bytes: &[u8], width: u8) {
        let high_bits = width % 8;
        let full_bytes = (width / 8) as usize;

        if high_bits > 0 {
            self.write_bits(bytes[full_bytes], high_bits);
        }
        for i in (0..full_bytes).rev() {
            self.write_byte(bytes[i]);
        }
    }

    pub fn write_int(&mut self, int: i64) {
        let header = INT_WIDTHS
            .iter()
            .position(|&width| width >= 64 || int < (1i64 << width))
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
        let mut bytes = string.as_bytes().iter().copied();

        let (bundles, remainder) = Self::string_bundles(string);

        // length header
        // write as bundles + remainder?
        let length = bundles * ASCII_BUNDLE_SIZE as usize + remainder;
        self.write_int(length as i64);

        let mut bundle_buffer = [0u64; ASCII_BUNDLE_SIZE as usize];
        for _ in 0..bundles {
            // get a chunk of ASCII_BUNDLE_SIZE
            for i in 0..ASCII_BUNDLE_SIZE as usize {
                let next_byte = bytes
                    .next()
                    .expect("should have a next byte in string while bundling");
                bundle_buffer[i] = compact_ascii(next_byte) as u64;
            }

            let bundle =
                ultra_packer::encode(ASCII_BUNDLE_SIZE, ASCII_MAX_VALUE as u64, &bundle_buffer);
            ultra_packer::write_bundle(self, ASCII_BITS_PER_BUNDLE, bundle);
        }

        // write remainder as smaller bundle
        if remainder > 0 {
            let mut remainder_buffer = vec![0u64; remainder];
            for i in 0..remainder {
                let byte = bytes.next().expect("should still have a remainder byte");
                remainder_buffer[i] = compact_ascii(byte) as u64;
            }

            let remainder_bits =
                ultra_packer::bits_per_bundle(ASCII_MAX_VALUE as u64, remainder as u8);
            let remainder_bundle =
                ultra_packer::encode(remainder as u8, ASCII_MAX_VALUE as u64, &remainder_buffer);
            ultra_packer::write_bundle(self, remainder_bits, remainder_bundle);
        }
    }

    fn string_bundles(string: &str) -> (usize, usize) {
        let bundles = string.len() / ASCII_BUNDLE_SIZE as usize;
        let remainder = string.len() % ASCII_BUNDLE_SIZE as usize;
        (bundles, remainder)
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

    fn advance(&mut self) {
        self.bit_offset += 1;
        if self.bit_offset == 8 {
            self.byte_index += 1;
            self.bit_offset = 0;
        }
    }

    pub fn read_bit(&mut self) -> Option<bool> {
        let byte = *self.buffer.get(self.byte_index)?;
        let bit = (byte >> (7 - self.bit_offset)) & 1 != 0;
        self.advance();
        Some(bit)
    }

    pub fn read_bits(&mut self, width: u8) -> Option<u8> {
        let space = 8 - self.bit_offset;
        let byte = *self.buffer.get(self.byte_index)?;
        let mask = ((1u16 << width) - 1) as u8;

        if width <= space {
            let result = (byte >> (space - width)) & mask;
            self.bit_offset += width;
            if self.bit_offset == 8 {
                self.byte_index += 1;
                self.bit_offset = 0;
            }
            Some(result)
        } else {
            let overflow = width - space;
            let first = byte & ((1u8 << space) - 1);
            self.byte_index += 1;
            let second = *self.buffer.get(self.byte_index)? >> (8 - overflow);
            self.bit_offset = overflow;
            Some((first << overflow) | second)
        }
    }

    pub fn read_byte(&mut self) -> Option<u8> {
        let byte = *self.buffer.get(self.byte_index)?;

        if self.bit_offset == 0 {
            self.byte_index += 1;
            Some(byte)
        } else {
            let space = 8 - self.bit_offset;
            self.byte_index += 1;
            let next = *self.buffer.get(self.byte_index)?;
            Some((byte << self.bit_offset) | (next >> space))
        }
    }

    pub fn read_bytes(&mut self, n: usize) -> Option<Vec<u8>> {
        let mut result = Vec::with_capacity(n);
        for _ in 0..n {
            result.push(self.read_byte()?);
        }
        Some(result)
    }

    pub fn read_bytes_width(&mut self, width: u8) -> Option<u64> {
        let high_bits = width % 8;
        let full_bytes = width / 8;

        let mut value: u64 = if high_bits > 0 {
            self.read_bits(high_bits)? as u64
        } else {
            0
        };

        for _ in 0..full_bytes {
            value = (value << 8) | (self.read_byte()? as u64);
        }
        Some(value)
    }

    pub fn read_int(&mut self) -> Option<i64> {
        let header = self.read_bits(3)?;
        let width = INT_WIDTHS[header as usize];
        Some(self.read_bytes_width(width)? as i64)
    }

    pub fn read_ascii_string(&mut self) -> Option<String> {
        let mut bytes = Vec::new();
        loop {
            let byte = self.read_bits(7)?;
            if byte == NULL_TERMINATOR {
                break;
            }

            bytes.push(byte);
        }

        // from_utf8_lossy_owned would be cool
        Some(String::from_utf8_lossy(bytes.as_slice()).into_owned())
    }

    pub fn read_ascii_string_ultrapacked(&mut self) -> Option<String> {
        let mut bytes = Vec::new();

        let length = self.read_int()?;
        let bundles = length as usize / ASCII_BUNDLE_SIZE as usize;
        let remainder = length as usize % ASCII_BUNDLE_SIZE as usize;

        for _ in 0..bundles {
            let bundle = ultra_packer::read_bundle(self, ASCII_BITS_PER_BUNDLE)?;
            let decoded = ultra_packer::decode(ASCII_BUNDLE_SIZE, ASCII_MAX_VALUE as u64, bundle);
            for byte in decoded {
                bytes.push(uncompact_ascii(byte as u8));
            }
        }

        // Read remainder as smaller bundle (matching write)
        if remainder > 0 {
            let remainder_bits =
                ultra_packer::bits_per_bundle(ASCII_MAX_VALUE as u64, remainder as u8);
            let remainder_bundle = ultra_packer::read_bundle(self, remainder_bits)?;
            let decoded =
                ultra_packer::decode(remainder as u8, ASCII_MAX_VALUE as u64, remainder_bundle);
            for byte in decoded {
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
            if byte == NULL_TERMINATOR {
                break;
            }

            bytes.push(byte);
        }

        // from_utf8_lossy_owned would be nice
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
    pub fn sanity() {
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
    pub fn sanity_int() {
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
