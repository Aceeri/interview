use std::borrow::Cow;

use crate::{huffman, serializer::PropertyType, ultra_packer};

// UTF8-style integer length
// prefix: 0, 10, 110, 1110, ...
// biased towards smaller values
const INT_WIDTHS: [u8; 7] = [3, 7, 9, 15, 24, 45, 64];

fn int_slot_width(int: i64) -> (usize, u8) {
    let slot = INT_WIDTHS
        .iter()
        .position(|&w| w >= 64 || int < (1i64 << w))
        .unwrap_or(INT_WIDTHS.len() - 1);
    (slot, INT_WIDTHS[slot])
}

pub fn int_encoded_bits(int: i64) -> u64 {
    let (slot, width) = int_slot_width(int);
    // prefix bits (slot 1s + terminating 0, unless last slot) + data bits
    let prefix_bits = if slot == INT_WIDTHS.len() - 1 {
        slot
    } else {
        slot + 1
    };
    prefix_bits as u64 + width as u64
}

// Character set bitflags for reducing possible values in packing
const CHARSETS: u8 = 4;
const CHARSET_UPPER: u8 = 1;
const CHARSET_LOWER: u8 = 1 << 1;
const CHARSET_NUMERAL: u8 = 1 << 2;
const CHARSET_RARE_PUNCT: u8 = 1 << 3;

const COMMON_PUNCT: &[u8] = b" ,-./:_"; // default charset
const LOWER_CASE: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const UPPER_CASE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &[u8] = b"0123456789";
const RARE_PUNCT: &[u8] = b"!\"#$%&'()*+;<=>?@[\\]^`{|}~"; // maybe ?!"' should be common?

fn is_common_punct(c: u8) -> bool {
    matches!(c, b' ' | b',' | b'-' | b'.' | b'/' | b':' | b'_')
}

pub fn detect_charset_flags(s: &str) -> u8 {
    let mut flags = 0u8;
    for &c in s.as_bytes() {
        match c {
            b'A'..=b'Z' => flags |= CHARSET_UPPER,
            b'a'..=b'z' => flags |= CHARSET_LOWER,
            b'0'..=b'9' => flags |= CHARSET_NUMERAL,
            _ if is_common_punct(c) => {} // common punct always included in default charset
            33..=47 | 58..=64 | 91..=96 | 123..=126 => flags |= CHARSET_RARE_PUNCT,
            _ => unreachable!("should filter non-ascii & 1-31/DEL"),
        }
    }
    flags
}

fn build_charset(flags: u8) -> Vec<u8> {
    let mut chars = Vec::new();

    chars.extend_from_slice(COMMON_PUNCT);

    if flags & CHARSET_LOWER != 0 {
        chars.extend_from_slice(LOWER_CASE);
    }

    if flags & CHARSET_NUMERAL != 0 {
        chars.extend_from_slice(DIGITS);
    }

    if flags & CHARSET_UPPER != 0 {
        chars.extend_from_slice(UPPER_CASE);
    }

    if flags & CHARSET_RARE_PUNCT != 0 {
        chars.extend_from_slice(RARE_PUNCT);
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

    pub fn write_bits_u16(&mut self, bits: u16, width: u8) {
        if width <= 8 {
            self.write_bits(bits as u8, width);
        } else {
            self.write_bits((bits >> 8) as u8, width - 8);
            self.write_byte(bits as u8);
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
        let (slot, width) = int_slot_width(int);

        // prefix: slot 1s followed by a 0 (unless last slot)
        for _ in 0..slot {
            self.write_bit(true);
        }
        if slot < INT_WIDTHS.len() - 1 {
            self.write_bit(false);
        }

        self.write_bytes_width(&int.to_le_bytes(), width);
    }

    pub fn write_ascii_string_adaptive(&mut self, string: &Cow<str>) {
        let charset_flags = detect_charset_flags(string);
        let ultrapack_bits = estimate_ultrapack_bits(string, charset_flags);
        let huffman_bits = estimate_huffman_bits(string);

        if huffman_bits < ultrapack_bits {
            self.write_bit(true); // 1 = huffman
            self.write_ascii_huffman_string(string);
        } else {
            self.write_bit(false); // 0 = ultrapack
            self.write_ascii_ultrapacked_string(string, charset_flags);
        }
    }

    pub fn write_ascii_ultrapacked_string(&mut self, string: &Cow<str>, charset_flags: u8) {
        let charset = build_charset(charset_flags);
        let max_value = charset.len() as u64;

        self.write_bits(charset_flags, CHARSETS);

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

    pub fn write_ascii_huffman_string(&mut self, string: &Cow<str>) {
        self.write_int(string.len() as i64);
        for &c in string.as_bytes() {
            if let Some(&(code, len)) = huffman::HUFFMAN_TABLE.get(&c) {
                self.write_bits_u16(code, len);
            } else {
                self.write_bits(c & 0x7F, 7);
            }
        }
    }

    pub fn write_unicode_huffman_string(&mut self, string: &Cow<str>) {
        self.write_int(string.len() as i64);
        for &c in string.as_bytes() {
            if let Some(&(code, len)) = huffman::HUFFMAN_TABLE.get(&c) {
                self.write_bit(false);
                self.write_bits_u16(code, len);
            } else {
                self.write_bit(true);
                self.write_byte(c);
            }
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

    pub fn read_ascii_ultrapacked_string(&mut self) -> Option<String> {
        let flags = self.read_bits(CHARSETS)?;
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

    fn read_huffman_byte(&mut self) -> Option<u8> {
        let (code, bits_read) = self.read_bits_u16_padded(huffman::HUFFMAN_MAX_LEN);

        if bits_read == 0 {
            return None;
        }

        let (character, actual_len) = huffman::HUFFMAN_DECODE[code as usize];

        if actual_len == 0 || actual_len > bits_read {
            return None;
        }

        self.rewind_bits(bits_read - actual_len);
        Some(character)
    }

    pub fn read_ascii_huffman_string(&mut self) -> Option<String> {
        let length = self.read_int()?;
        if length < 0 {
            return None;
        }
        let length = length as usize;
        let mut bytes = Vec::with_capacity(length);

        for _ in 0..length {
            bytes.push(self.read_huffman_byte()?);
        }

        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    pub fn read_unicode_huffman_string(&mut self) -> Option<String> {
        let length = self.read_int()? as usize;
        let mut bytes = Vec::with_capacity(length);

        for _ in 0..length {
            let is_escaped = self.read_bit()?;

            if is_escaped {
                bytes.push(self.read_byte()?);
            } else {
                bytes.push(self.read_huffman_byte()?);
            }
        }

        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    pub fn read_property_type(&mut self) -> Option<PropertyType> {
        let bits = self.read_bits(2)?;
        PropertyType::from_bits(bits)
    }

    pub fn rewind_bits(&mut self, bits: u8) {
        let total_bits = self.byte_index * 8 + self.bit_offset as usize;
        let new_total = total_bits.saturating_sub(bits as usize);
        self.byte_index = new_total / 8;
        self.bit_offset = (new_total % 8) as u8;
    }

    pub fn read_bits_u16_padded(&mut self, width: u8) -> (u16, u8) {
        let mut value: u16 = 0;
        let mut bits_read: u8 = 0;

        for _ in 0..width {
            match self.read_bit() {
                Some(bit) => {
                    value = (value << 1) | (bit as u16);
                    bits_read += 1;
                }
                None => {
                    // Pad remaining bits with zeros
                    value <<= width - bits_read;
                    break;
                }
            }
        }

        (value, bits_read)
    }
}

pub fn estimate_ultrapack_bits(string: &str, charset_flags: u8) -> u64 {
    let charset = build_charset(charset_flags);
    let max_value = charset.len() as u64;
    let (bundle_size, bits_per_bundle) = ultra_packer::find_optimal_bundle(max_value);

    let bundles = string.len() / bundle_size as usize;
    let remainder = string.len() % bundle_size as usize;

    // 1 bit selector + header + length prefix + content
    let mut bits = 1 + CHARSETS as u64;
    bits += int_encoded_bits(string.len() as i64);
    bits += bundles as u64 * bits_per_bundle as u64;
    if remainder > 0 {
        bits += ultra_packer::bits_per_bundle(max_value, remainder as u8) as u64;
    }
    bits
}

pub fn estimate_huffman_bits(string: &str) -> u64 {
    // 1 bit selector + length prefix + huffman codes
    let mut bits = 1 + int_encoded_bits(string.len() as i64);
    for &c in string.as_bytes() {
        if let Some(&(_, len)) = huffman::HUFFMAN_TABLE.get(&c) {
            bits += len as u64;
        } else {
            bits += 7; // fallback for chars not in table
        }
    }
    bits
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
    pub fn sanity() {
        let mut buffer = Vec::new();
        let mut packer = BitPacker::new(&mut buffer);

        packer.write_bits(0b101, 3);
        packer.write_bits(0b11110000, 8);
        packer.write_bit(true);

        let mut unpacker = BitUnpacker::new(&buffer);
        assert_eq!(unpacker.read_bits(3), Some(0b101));
        assert_eq!(unpacker.read_bits(8), Some(0b11110000));
        assert_eq!(unpacker.read_bit(), Some(true));
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
