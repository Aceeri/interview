use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::bit_packer::{BitPacker, BitUnpacker};

const MAX_CODE_LEN: u8 = 15;

#[derive(Clone)]
struct HuffNode {
    freq: u64,
    symbol: Option<u8>,
    left: Option<Box<HuffNode>>,
    right: Option<Box<HuffNode>>,
}

impl Eq for HuffNode {}
impl PartialEq for HuffNode {
    fn eq(&self, other: &Self) -> bool {
        self.freq == other.freq
    }
}
impl Ord for HuffNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other.freq.cmp(&self.freq)
    }
}
impl PartialOrd for HuffNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub struct HuffmanTable {
    codes: [u32; 256],
    lengths: [u8; 256],
    decode_table: Vec<u8>,
    decode_lengths: Vec<u8>,
}

impl HuffmanTable {
    pub fn from_counts(counts: &[u64; 256]) -> Self {
        let mut codes = [0u32; 256];
        let mut lengths = [0u8; 256];

        let mut heap = BinaryHeap::new();
        for (symbol, &freq) in counts.iter().enumerate() {
            if freq > 0 {
                heap.push(HuffNode {
                    freq,
                    symbol: Some(symbol as u8),
                    left: None,
                    right: None,
                });
            }
        }

        if heap.is_empty() {
            return Self::build_table(codes, lengths);
        }

        if heap.len() == 1 {
            let node = heap.pop().unwrap();
            if let Some(sym) = node.symbol {
                codes[sym as usize] = 0;
                lengths[sym as usize] = 1;
            }
            return Self::build_table(codes, lengths);
        }

        while heap.len() > 1 {
            let left = heap.pop().unwrap();
            let right = heap.pop().unwrap();
            heap.push(HuffNode {
                freq: left.freq + right.freq,
                symbol: None,
                left: Some(Box::new(left)),
                right: Some(Box::new(right)),
            });
        }

        fn traverse(node: &HuffNode, depth: u8, lengths: &mut [u8; 256]) {
            if let Some(sym) = node.symbol {
                lengths[sym as usize] = depth.min(MAX_CODE_LEN);
            } else {
                if let Some(ref left) = node.left {
                    traverse(left, depth + 1, lengths);
                }
                if let Some(ref right) = node.right {
                    traverse(right, depth + 1, lengths);
                }
            }
        }

        traverse(&heap.pop().unwrap(), 0, &mut lengths);
        Self::limit_lengths(&mut lengths);
        Self::build_codes(&mut codes, &lengths);
        Self::build_table(codes, lengths)
    }

    fn limit_lengths(lengths: &mut [u8; 256]) {
        for len in lengths.iter_mut() {
            if *len > MAX_CODE_LEN {
                *len = MAX_CODE_LEN;
            }
        }

        loop {
            let mut kraft_sum: u64 = 0;
            for &len in lengths.iter() {
                if len > 0 {
                    kraft_sum += 1u64 << (MAX_CODE_LEN - len);
                }
            }
            if kraft_sum <= 1u64 << MAX_CODE_LEN {
                break;
            }
            let mut min_len = MAX_CODE_LEN + 1;
            let mut min_idx = 0;
            for (i, &len) in lengths.iter().enumerate() {
                if len > 0 && len < min_len {
                    min_len = len;
                    min_idx = i;
                }
            }
            if min_len <= MAX_CODE_LEN {
                lengths[min_idx] += 1;
            } else {
                break;
            }
        }
    }

    fn build_codes(codes: &mut [u32; 256], lengths: &[u8; 256]) {
        let mut bl_count = [0u32; (MAX_CODE_LEN + 1) as usize];
        for &len in lengths.iter() {
            if len > 0 && len <= MAX_CODE_LEN {
                bl_count[len as usize] += 1;
            }
        }

        let mut next_code = [0u32; (MAX_CODE_LEN + 1) as usize];
        let mut code = 0u32;
        for bits in 1..=MAX_CODE_LEN {
            code = (code + bl_count[(bits - 1) as usize]) << 1;
            next_code[bits as usize] = code;
        }

        for sym in 0..256 {
            let len = lengths[sym];
            if len > 0 && len <= MAX_CODE_LEN {
                codes[sym] = next_code[len as usize];
                next_code[len as usize] += 1;
            }
        }
    }

    fn build_table(codes: [u32; 256], lengths: [u8; 256]) -> Self {
        let table_size = 1usize << MAX_CODE_LEN;
        let mut decode_table = vec![0u8; table_size];
        let mut decode_lengths = vec![0u8; table_size];

        for sym in 0u16..256 {
            let len = lengths[sym as usize];
            if len > 0 && len <= MAX_CODE_LEN {
                let code = codes[sym as usize];
                let shift = MAX_CODE_LEN - len;
                let base = (code as usize) << shift;
                let count = 1usize << shift;
                for i in 0..count {
                    decode_table[base + i] = sym as u8;
                    decode_lengths[base + i] = len;
                }
            }
        }

        HuffmanTable {
            codes,
            lengths,
            decode_table,
            decode_lengths,
        }
    }

    // This could easily be improved given some real world data on text frequencies.
    pub fn common_table() -> Self {
        let mut counts = [0u64; 256];
        counts[b' ' as usize] = 18000;
        counts[b'e' as usize] = 10000;
        counts[b't' as usize] = 7000;
        counts[b'a' as usize] = 6500;
        counts[b'o' as usize] = 6000;
        counts[b'i' as usize] = 5500;
        counts[b'n' as usize] = 5500;
        counts[b's' as usize] = 5000;
        counts[b'h' as usize] = 4800;
        counts[b'r' as usize] = 4700;
        counts[b'd' as usize] = 3500;
        counts[b'l' as usize] = 3500;
        counts[b'c' as usize] = 2500;
        counts[b'u' as usize] = 2500;
        counts[b'm' as usize] = 2000;
        counts[b'w' as usize] = 2000;
        counts[b'f' as usize] = 1800;
        counts[b'g' as usize] = 1600;
        counts[b'y' as usize] = 1600;
        counts[b'p' as usize] = 1500;
        counts[b'b' as usize] = 1200;
        counts[b'v' as usize] = 800;
        counts[b'k' as usize] = 600;

        counts[b'/' as usize] = 400;
        counts[b'-' as usize] = 400;
        counts[b'_' as usize] = 400;
        counts[b'.' as usize] = 400;

        counts[b'[' as usize] = 300;
        counts[b']' as usize] = 300;
        counts[b'{' as usize] = 300;
        counts[b'}' as usize] = 300;
        counts[b'(' as usize] = 300;
        counts[b')' as usize] = 300;
        counts[b'\\' as usize] = 300;

        counts[b'j' as usize] = 100;
        counts[b'x' as usize] = 100;
        counts[b'q' as usize] = 80;
        counts[b'z' as usize] = 60;

        //counts[0] = 500; // null terminators?

        for i in 32..127 {
            // fill any other common ASCII
            if counts[i] == 0 {
                counts[i] = 200;
            }
        }

        // maybe increase counts for 10XXXXXX values over
        // 11XXXXXX values? If we are in continuation byte territory its probably more likely we are
        // in the 3/4ths of the unicode that is 10 prefixed rather than 110, 1110, 11110 prefixed
        // for i in 0b10000000..0b10111111 {
        //     if counts[i] == 0 {
        //         counts[i] = 50;
        //     }
        // }

        Self::from_counts(&counts)
    }

    #[inline]
    fn get_code(&self, symbol: u8) -> (u32, u8) {
        (self.codes[symbol as usize], self.lengths[symbol as usize])
    }
}

pub fn compress(data: &[u8], table: &HuffmanTable) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut packer = BitPacker::new(&mut buffer);
    for &byte in data {
        let (code, len) = table.get_code(byte);
        packer.write_bits_u32(code, len);
    }
    packer.finish()
}

pub fn decompress(compressed: &[u8], length: usize, table: &HuffmanTable) -> Vec<u8> {
    let mut unpacker = BitUnpacker::new(compressed);
    let mut result = Vec::with_capacity(length);
    for _ in 0..length {
        let bits = unpacker.peek_bits(MAX_CODE_LEN);
        let symbol = table.decode_table[bits];
        let len = table.decode_lengths[bits];
        unpacker.skip_bits(len);
        result.push(symbol);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_uniform() {
        let data = b"hello world";
        let mut counts = [0u64; 256];
        for &b in data {
            counts[b as usize] += 1;
        }
        let table = HuffmanTable::from_counts(&counts);
        let compressed = compress(data, &table);
        let decompressed = decompress(&compressed, data.len(), &table);
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_roundtrip_table() {
        let data = b"the quick brown fox jumps over the lazy dog";
        let table = HuffmanTable::common_table();
        let compressed = compress(data, &table);
        let decompressed = decompress(&compressed, data.len(), &table);
        assert_eq!(decompressed, data);
        println!(
            "Huffman: {} bytes -> {} bytes",
            data.len(),
            compressed.len()
        );
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_compression_ratio() {
        let data = b"this is a test of the emergency broadcast system. \
                     this is only a test. if this had been an actual emergency, \
                     you would have been instructed where to tune in your area.";
        let table = HuffmanTable::common_table();
        let compressed = compress(data, &table);
        let decompressed = decompress(&compressed, data.len(), &table);
        assert_eq!(decompressed, data);
        let ratio = compressed.len() as f64 / data.len() as f64;
        println!(
            "Huffman: {} bytes -> {} bytes ({:.1}%)",
            data.len(),
            compressed.len(),
            ratio * 100.0
        );
    }

    #[test]
    fn test_repetitive_data() {
        let data = b"aaaaaaaaaaaabbbbbbccccddddeeeeee";
        let mut counts = [0u64; 256];
        for &b in data {
            counts[b as usize] += 1;
        }
        let table = HuffmanTable::from_counts(&counts);
        let compressed = compress(data, &table);
        let decompressed = decompress(&compressed, data.len(), &table);
        assert_eq!(decompressed, data);
        println!(
            "Repetitive: {} bytes -> {} bytes",
            data.len(),
            compressed.len()
        );
    }
}
