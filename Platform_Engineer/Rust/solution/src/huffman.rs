use std::collections::HashMap;
use std::sync::LazyLock;

// Character frequencies tuned for config/metadata strings
// Hand-tuned based on: EXIF metadata, file paths, game saves, API configs,
// editor settings, package.json, k8s manifests, .env files, numeric data
const CHAR_FREQUENCIES: &[(u8, u32)] = &[
    // Lowercase (tuned for config patterns)
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
    // Punctuation (config-specific)
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
    // Uppercase (common in configs: ENV_VARS, constants)
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

fn build_optimal_lengths(freq: &[(u8, u32)], max_len: u8) -> Vec<(u8, u8)> {
    if freq.is_empty() {
        return vec![];
    }

    // Sort by frequency (descending) for better initial assignment
    let mut sorted: Vec<(u8, u32)> = freq.to_vec();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    // Calculate ideal lengths: l_i = -log2(p_i) where p_i = freq_i / total
    let total: u64 = sorted.iter().map(|(_, f)| *f as u64).sum();
    if total == 0 {
        return sorted.iter().map(|&(c, _)| (c, max_len)).collect();
    }

    let mut lengths: Vec<(u8, u8, f64)> = sorted
        .iter()
        .map(|&(c, f)| {
            let p = f as f64 / total as f64;
            let ideal_len = if p > 0.0 { -p.log2() } else { max_len as f64 };
            (c, ideal_len.ceil().min(max_len as f64) as u8, p)
        })
        .collect();

    // Adjust lengths to satisfy Kraft inequality: Σ 2^(-len) <= 1
    loop {
        let kraft_sum: f64 = lengths
            .iter()
            .map(|(_, l, _)| 2.0_f64.powi(-(*l as i32)))
            .sum();

        if kraft_sum <= 1.0 + 1e-9 {
            break;
        }

        // Find symbol with shortest length that can be increased
        if let Some(idx) = lengths
            .iter()
            .enumerate()
            .filter(|(_, (_, l, _))| *l < max_len)
            .min_by(|(_, (_, _, p1)), (_, (_, _, p2))| p1.partial_cmp(p2).unwrap())
            .map(|(i, _)| i)
        {
            lengths[idx].1 += 1;
        } else {
            break;
        }
    }

    // Try to use unused Kraft slack by shortening high-frequency codes
    loop {
        let kraft_sum: f64 = lengths
            .iter()
            .map(|(_, l, _)| 2.0_f64.powi(-(*l as i32)))
            .sum();
        let slack = 1.0 - kraft_sum;

        // Find symbol that would benefit most from shorter code
        if let Some(idx) = lengths
            .iter()
            .enumerate()
            .filter(|(_, (_, l, _))| *l > 1)
            .filter(|(_, (_, l, _))| {
                2.0_f64.powi(-((*l - 1) as i32)) - 2.0_f64.powi(-(*l as i32)) <= slack + 1e-9
            })
            .max_by(|(_, (_, _, p1)), (_, (_, _, p2))| p1.partial_cmp(p2).unwrap())
            .map(|(i, _)| i)
        {
            lengths[idx].1 -= 1;
        } else {
            break;
        }
    }

    lengths.into_iter().map(|(c, l, _)| (c, l)).collect()
}

fn build_canonical_codes(lengths: &[(u8, u8)]) -> HashMap<u8, (u16, u8)> {
    let mut table = HashMap::new();

    if lengths.is_empty() {
        return table;
    }

    // Sort by length, then by symbol for canonical ordering
    let mut sorted: Vec<(u8, u8)> = lengths.to_vec();
    sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

    let max_len = sorted.iter().map(|&(_, l)| l).max().unwrap_or(0);

    // Count symbols at each length
    let mut bl_count = vec![0u32; max_len as usize + 1];
    for &(_, len) in &sorted {
        bl_count[len as usize] += 1;
    }

    // Compute first code for each length
    let mut next_code = vec![0u16; max_len as usize + 1];
    let mut code = 0u16;
    for bits in 1..=max_len {
        code = (code + bl_count[bits as usize - 1] as u16) << 1;
        next_code[bits as usize] = code;
    }

    // Assign codes to symbols
    for &(ch, len) in &sorted {
        table.insert(ch, (next_code[len as usize], len));
        next_code[len as usize] += 1;
    }

    table
}

pub const HUFFMAN_MAX_LEN: u8 = 12;
fn build_decode_table(encode_table: &HashMap<u8, (u16, u8)>) -> Vec<(u8, u8)> {
    let table_size = 1usize << HUFFMAN_MAX_LEN;
    let mut table = vec![(0u8, 0u8); table_size];

    for (&ch, &(code, len)) in encode_table {
        // Number of entries this code covers (all suffixes)
        let suffix_count = 1usize << (HUFFMAN_MAX_LEN - len);
        // Left-align the code to max_len bits
        let base_index = (code as usize) << (HUFFMAN_MAX_LEN - len);

        for suffix in 0..suffix_count {
            table[base_index | suffix] = (ch, len);
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
