use crate::bit_packer::{BitPacker, BitUnpacker};

// Was curious to see if there was a way to utilize the wasted space that a lot of
// compacting runs into. If values don't match up with a power of two there is inherent waste.
//
// Combining values seems like a common way to get around this. Kind of see it a bit like
// arithmetic encoding or linearization of multidimensional arrays.
//
// e.g.
//
// 3d array of 18x18x18 will be 5832 elements, indexing this via each axis like:
// - needs 2^5, but that wastes 14 values per axis, 43.75% of the space allocated to it.
// indexing it via a simple linearization scheme: x + y * 18 + z * 18 * 18
// means we only need a number for `5832` which requires 13 bits savings 2 bits.
//
//
// `UltraPacker` is a mostly an abstraction of this concept, where 18 is the max value,
// 3 is the bundle size and 13 bits is the bits per bundle.
//
// https://save-buffer.github.io/ultrapack.html

pub const fn bits_per_bundle(max_value: u64, bundle_size: u8) -> u8 {
    let max_bundle = max_value.pow(bundle_size as u32);
    (64 - (max_bundle - 1).leading_zeros()) as u8
}

pub const fn find_optimal_bundle(max_value: u64) -> (u8, u8) {
    assert!(max_value > 0);
    let naive_bits = max_value.ilog2() + 1;

    let mut best_size = 1u8;
    let mut best_bits_per_val = naive_bits as f64;

    // test bundle sizes until we overflow u64
    let mut bundle_size = 1;
    while bundle_size <= 40u8 {
        // max_value^k - 1
        let Some(max_bundle) = max_value.checked_pow(bundle_size as u32) else {
            break;
        };

        let bits_needed = (64 - (max_bundle - 1).leading_zeros()) as u8;
        let bits_per_val = bits_needed as f64 / bundle_size as f64;

        if bits_per_val < best_bits_per_val {
            best_bits_per_val = bits_per_val;
            best_size = bundle_size;
        }

        bundle_size += 1;
    }

    (best_size, bits_per_bundle(max_value, best_size))
}

pub fn encode(bundle_size: u8, max_value: u64, values: &[u64]) -> u64 {
    assert_eq!(values.len(), bundle_size as usize);

    let mut bundle: u64 = 0;
    for &val in values {
        assert!(val < max_value);
        bundle = bundle * max_value + val;
    }
    bundle
}

pub fn decode(bundle_size: u8, max_value: u64, mut bundle: u64) -> Vec<u64> {
    let mut values = vec![0u64; bundle_size as usize];

    for i in (0..bundle_size as usize).rev() {
        values[i] = bundle % max_value;
        bundle /= max_value;
    }

    values
}

pub fn write_bundle(packer: &mut BitPacker, bits_per_bundle: u8, bundle: u64) {
    let bytes = bundle.to_le_bytes();
    packer.write_bytes_width(&bytes, bits_per_bundle);
}

pub fn read_bundle(unpacker: &mut BitUnpacker, bits_per_bundle: u8) -> Option<u64> {
    unpacker.read_bytes_width(bits_per_bundle)
}
