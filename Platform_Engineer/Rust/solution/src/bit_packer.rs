use std::borrow::Cow;

use crate::{serializer::PropertyType, ultra_packer};

/// UTF8-style integer length
/// prefix: 0, 10, 110, 1110, 11110, 111110, 1111110
/// widths: 3,  7,   9,   15,    24,     45,      64
/// biased towards smaller values
const INT_WIDTHS: [u8; 7] = [3, 7, 9, 15, 24, 45, 64];
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

// Character set bitflags for reducing possible values in packing
const CHARSET_UPPER: u8 = 1 << 4;
const CHARSET_LOWER: u8 = 1 << 3;
const CHARSET_NUMERAL: u8 = 1 << 2;
const CHARSET_COMMON_PUNCT: u8 = 1 << 1; // . , / - _ :
const CHARSET_RARE_PUNCT: u8 = 1;

fn is_common_punct(c: u8) -> bool {
    matches!(c, b'.' | b',' | b'/' | b'-' | b'_' | b':')
}

fn detect_charset_flags(s: &str) -> u8 {
    let mut flags = 0u8;
    for &c in s.as_bytes() {
        match c {
            b'A'..=b'Z' => flags |= CHARSET_UPPER,
            b'a'..=b'z' => flags |= CHARSET_LOWER,
            b'0'..=b'9' => flags |= CHARSET_NUMERAL,
            b' ' => {} // space always included, doesn't set a flag
            _ if is_common_punct(c) => flags |= CHARSET_COMMON_PUNCT,
            33..=47 | 58..=64 | 91..=96 | 123..=126 => flags |= CHARSET_RARE_PUNCT,
            _ => unreachable!("should filter non-ascii & 1-31/DEL"),
        }
    }
    flags
}

fn build_charset(flags: u8) -> Vec<u8> {
    let mut chars = vec![b' ']; // space always first

    if flags & CHARSET_UPPER != 0 {
        for c in b'A'..=b'Z' {
            chars.push(c);
        }
    }

    if flags & CHARSET_LOWER != 0 {
        for c in b'a'..=b'z' {
            chars.push(c);
        }
    }

    if flags & CHARSET_NUMERAL != 0 {
        for c in b'0'..=b'9' {
            chars.push(c);
        }
    }

    if flags & CHARSET_COMMON_PUNCT != 0 {
        for c in [b'.', b',', b'-', b'/', b':', b'_'] {
            chars.push(c);
        }
    }

    if flags & CHARSET_RARE_PUNCT != 0 {
        for c in 33u8..=43 {
            chars.push(c);
        }
        for c in 59u8..=64 {
            chars.push(c);
        }
        for c in 91u8..=94 {
            chars.push(c);
        }
        chars.push(96);
        for c in 123u8..=126 {
            chars.push(c);
        }
    }

    chars
}

fn compact_charset(c: u8, charset: &[u8]) -> u8 {
    charset
        .iter()
        .position(|&x| x == c)
        .expect("char not in charset") as u8
}

fn uncompact_charset(idx: u8, charset: &[u8]) -> u8 {
    charset[idx as usize]
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
        let slot = INT_WIDTHS
            .iter()
            .position(|&w| w >= 64 || int < (1i64 << w))
            .unwrap_or(INT_WIDTHS.len() - 1);
        let width = INT_WIDTHS[slot];

        // prefix
        for _ in 0..slot {
            self.write_bit(true);
        }
        if slot < INT_WIDTHS.len() - 1 {
            self.write_bit(false);
        }

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
    pub fn write_ascii_string_charset(&mut self, string: &Cow<str>) {
        let charset_flags = detect_charset_flags(string);
        let charset = build_charset(charset_flags);
        let max_value = charset.len() as u64;

        self.write_bits(charset_flags, 5);

        // TODO: make a LUT for every possible max value
        // 2^5 = 32 possible
        let (bundle_size, bits_per_bundle) = ultra_packer::find_optimal_bundle(max_value);

        let mut bytes = string.bytes();
        let bundles = string.len() / bundle_size as usize;
        let remainder = string.len() % bundle_size as usize;

        let length = bundles * bundle_size as usize + remainder;
        self.write_int(length as i64);

        let mut bundle_buffer = vec![0u64; bundle_size as usize];
        for _ in 0..bundles {
            for i in 0..bundle_size as usize {
                let byte = bytes.next().expect("should have another byte for bundle");
                bundle_buffer[i] = compact_charset(byte, &charset) as u64;
            }
            let bundle = ultra_packer::encode(bundle_size, max_value, &bundle_buffer);
            ultra_packer::write_bundle(self, bits_per_bundle, bundle);
        }

        if remainder > 0 {
            let mut remainder_buffer = vec![0u64; remainder];
            for i in 0..remainder {
                let byte = bytes
                    .next()
                    .expect("should have another byte for remainder");
                remainder_buffer[i] = compact_charset(byte, &charset) as u64;
            }
            let remainder_bits = ultra_packer::bits_per_bundle(max_value, remainder as u8);
            let remainder_bundle =
                ultra_packer::encode(remainder as u8, max_value, &remainder_buffer);
            ultra_packer::write_bundle(self, remainder_bits, remainder_bundle);
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
        // Count leading 1s to determine slot
        let mut slot = 0;
        while slot < 6 && self.read_bit()? {
            slot += 1;
        }

        let width = INT_WIDTHS[slot];
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

    /// Read string using charset-based ultrapacking
    pub fn read_ascii_string_ultrapacked(&mut self) -> Option<String> {
        // Read 5-bit charset flags
        let flags = self.read_bits(5)?;
        let charset = build_charset(flags);
        let max_value = charset.len() as u64;

        let (bundle_size, bits_per_bundle) = ultra_packer::find_optimal_bundle(max_value);

        let length = self.read_int()? as usize;
        let bundles = length / bundle_size as usize;
        let remainder = length % bundle_size as usize;

        let mut bytes = Vec::with_capacity(length);
        for _ in 0..bundles {
            let bundle = ultra_packer::read_bundle(self, bits_per_bundle)?;
            let decoded = ultra_packer::decode(bundle_size, max_value, bundle);
            for idx in decoded {
                bytes.push(uncompact_charset(idx as u8, &charset));
            }
        }

        if remainder > 0 {
            let remainder_bits = ultra_packer::bits_per_bundle(max_value, remainder as u8);
            let remainder_bundle = ultra_packer::read_bundle(self, remainder_bits)?;
            let decoded = ultra_packer::decode(remainder as u8, max_value, remainder_bundle);
            for idx in decoded {
                bytes.push(uncompact_charset(idx as u8, &charset));
            }
        }

        Some(String::from_utf8_lossy(&bytes).into_owned())
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
