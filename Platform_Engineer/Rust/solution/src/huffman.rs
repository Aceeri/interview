use std::collections::HashMap;
use std::sync::LazyLock;

const CHAR_FREQUENCIES: &[(u8, u32)] = &[
    // Lowercase
    (b'e', 710),
    (b'o', 500),
    (b'a', 470),
    (b'n', 460),
    (b't', 460),
    (b'i', 410),
    (b'r', 380),
    (b's', 370),
    (b'p', 290),
    (b'c', 240),
    (b'l', 240),
    (b'd', 180),
    (b'm', 160),
    (b'u', 120),
    (b'g', 100),
    (b'f', 100),
    (b'v', 90),
    (b'h', 70),
    (b'k', 60),
    (b'y', 50),
    (b'j', 50),
    (b'w', 40),
    (b'b', 30),
    (b'z', 20),
    (b'q', 10),
    // Digits (benfords law guessing + put ahead of capitals/punctuation)
    (b'0', 650),
    (b'1', 360),
    (b'2', 240),
    (b'3', 180),
    (b'4', 160),
    (b'5', 140),
    (b'6', 130),
    (b'7', 120),
    (b'8', 100),
    (b'9', 90),
    // Punctuation
    (b'.', 330),
    (b' ', 200),
    (b'/', 200),
    (b':', 180),
    (b'_', 180),
    (b'=', 70),
    (b'-', 60),
    (b',', 10),
    (b'(', 10),
    (b')', 10),
    (b'~', 10),
    (b'+', 10),
    // Uppercase
    (b'E', 230),
    (b'O', 160),
    (b'S', 160),
    (b'T', 150),
    (b'C', 130),
    (b'I', 90),
    (b'N', 90),
    (b'P', 80),
    (b'D', 80),
    (b'L', 70),
    (b'M', 70),
    (b'A', 70),
    (b'K', 60),
    (b'R', 60),
    (b'x', 70),
    (b'B', 50),
    (b'G', 40),
    (b'H', 40),
    (b'V', 30),
    (b'U', 30),
    (b'J', 20),
    (b'X', 20),
    (b'F', 20),
    (b'Y', 20),
    (b'W', 15),
    (b'Q', 5),
    (b'Z', 5),
    // Rare punctuation
    (b';', 8),
    (b'!', 5),
    (b'?', 5),
    (b'\'', 15),
    (b'"', 10),
    (b'[', 8),
    (b']', 8),
    (b'{', 6),
    (b'}', 6),
    (b'<', 6),
    (b'>', 6),
    (b'*', 6),
    (b'&', 5),
    (b'%', 5),
    (b'$', 4),
    (b'#', 5),
    (b'@', 6),
    (b'^', 3),
    (b'`', 3),
    (b'|', 5),
    (b'\\', 8),
];

struct Symbol {
    byte: u8,
    len: u8,
    probability: f64,
}

impl Symbol {
    fn kraft_contribution(&self) -> f64 {
        2.0_f64.powi(-(self.len as i32))
    }

    fn kraft_cost_to_shorten(&self) -> f64 {
        2.0_f64.powi(-((self.len - 1) as i32)) - self.kraft_contribution()
    }
}

fn kraft_sum(symbols: &[Symbol]) -> f64 {
    symbols.iter().map(|s| s.kraft_contribution()).sum()
}

fn build_optimal_lengths(frequencies: &[(u8, u32)], max_len: u8) -> Vec<(u8, u8)> {
    let total: u64 = frequencies.iter().map(|(_, f)| *f as u64).sum();
    if total == 0 {
        return frequencies.iter().map(|&(b, _)| (b, max_len)).collect();
    }

    let mut symbols: Vec<Symbol> = frequencies
        .iter()
        .map(|&(byte, freq)| {
            let probability = freq as f64 / total as f64;
            let ideal_len = (-probability.log2()).ceil().min(max_len as f64) as u8;
            Symbol {
                byte,
                len: ideal_len,
                probability,
            }
        })
        .collect();

    symbols.sort_by(|a, b| b.probability.partial_cmp(&a.probability).unwrap());

    while kraft_sum(&symbols) > 1.0 + 1e-9 {
        let lowest_prob_shortenable = symbols
            .iter_mut()
            .filter(|s| s.len < max_len)
            .min_by(|a, b| a.probability.partial_cmp(&b.probability).unwrap());

        match lowest_prob_shortenable {
            Some(symbol) => symbol.len += 1,
            None => break,
        }
    }

    loop {
        let slack = 1.0 - kraft_sum(&symbols);

        let best_to_shorten = symbols
            .iter_mut()
            .filter(|s| s.len > 1 && s.kraft_cost_to_shorten() <= slack + 1e-9)
            .max_by(|a, b| a.probability.partial_cmp(&b.probability).unwrap());

        match best_to_shorten {
            Some(symbol) => symbol.len -= 1,
            None => break,
        }
    }

    symbols.into_iter().map(|s| (s.byte, s.len)).collect()
}

fn build_canonical_codes(lengths: &[(u8, u8)]) -> HashMap<u8, (u16, u8)> {
    if lengths.is_empty() {
        return HashMap::new();
    }

    let mut symbols: Vec<(u8, u8)> = lengths.to_vec();
    symbols.sort_by_key(|&(byte, len)| (len, byte));

    let max_len = symbols.iter().map(|&(_, len)| len).max().unwrap() as usize;

    let mut count_at_length = vec![0u16; max_len + 1];
    for &(_, len) in &symbols {
        count_at_length[len as usize] += 1;
    }

    let mut first_code_at_length = vec![0u16; max_len + 1];
    for len in 1..=max_len {
        first_code_at_length[len] = (first_code_at_length[len - 1] + count_at_length[len - 1]) << 1;
    }

    let mut next_code = first_code_at_length;
    let mut table = HashMap::new();
    for (byte, len) in symbols {
        let code = next_code[len as usize];
        table.insert(byte, (code, len));
        next_code[len as usize] += 1;
    }

    table
}

pub const HUFFMAN_MAX_LEN: u8 = 12;

// build a LUT of every u16 that matches the 12 bit suffix
// basically just fill the last 4 bits with every possibility
// e.g.
// 0b10011011_110000 => 'e'
// 0b10011011_110001 => 'e'
// 0b10011011_110010 => 'e'
// ...
fn build_decode_table(encode_table: &HashMap<u8, (u16, u8)>) -> Vec<(u8, u8)> {
    let table_size = 1usize << HUFFMAN_MAX_LEN;
    let mut table = vec![(0u8, 0u8); table_size];

    for (&character, &(code, len)) in encode_table {
        let suffix_count = 1usize << (HUFFMAN_MAX_LEN - len);
        let base_index = (code as usize) << (HUFFMAN_MAX_LEN - len);

        for suffix in 0..suffix_count {
            table[base_index | suffix] = (character, len);
        }
    }

    table
}

pub static HUFFMAN_TABLE: LazyLock<HashMap<u8, (u16, u8)>> = LazyLock::new(|| {
    let lengths = build_optimal_lengths(CHAR_FREQUENCIES, 12);
    build_canonical_codes(&lengths)
});

/// index with max_len bits, get (char, actual_length)
pub static HUFFMAN_DECODE: LazyLock<Vec<(u8, u8)>> =
    LazyLock::new(|| build_decode_table(&HUFFMAN_TABLE));
