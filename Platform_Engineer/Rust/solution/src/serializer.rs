use std::collections::VecDeque;

use crate::bit_packer::{BitPacker, BitUnpacker};

#[derive(Debug, Default)]
pub struct Serializer<'a> {
    // each property is order-dependent, arrays are flattened into this structure and theoretically
    // nested structs would do the same.

    // we keep similar types next to each other since we can make better assumptions about the data next to
    // eachother for each type.

    // Main assumption: these are likely to be very small integers and fairly similar to eachother
    //
    // 2 bit length header for integers describing how many bytes it is
    // 00 - 1 byte, 01 - 2 bytes, 10 - 3 bytes, 11 - 4 bytes
    // delta encoding? Maybe use FastPFOR if I wanted to bring in a library
    integers: Vec<i64>,
    // UTF-8 is fairly compact already, just write that to the buffer. Delta encoding might be the worthwhile here
    // too for compression assuming its mostly alphanumeric.
    //
    // nul terminating the strings is slightly dangerous, but saves us from a length prefix.
    // if we wanted to skip serializing portions, we'd want some sort of index for each property, which would
    // then double as a length.
    //
    // if just ascii is enough, then we could compact to 7 bits easily
    // 1-32 are also unused which means we are still only using 75% of the values, so this could be compressed
    // further if we are given a sequence of strings
    strings: Vec<&'a str>,
    // booleans can just be bitpacked directly, hard to make further assumptions
    booleans: Vec<bool>,
}

// get the compiler to re-use the allocated Vec
// worst case the optimization fails and we end up with the naive allocating solution.
fn reuse_vec<T, U>(mut v: Vec<T>) -> Vec<U> {
    const {
        assert!(size_of::<T>() == size_of::<U>());
        assert!(align_of::<T>() == align_of::<U>());
    }
    v.clear();
    v.into_iter().map(|_| unreachable!()).collect()
}

impl<'a> Serializer<'a> {
    pub fn new() -> Self {
        Self {
            integers: Vec::new(),
            strings: Vec::new(),
            booleans: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.integers.clear();
        self.strings.clear();
        self.booleans.clear();
    }

    // for buffer re-use
    pub fn reuse<'b>(mut self) -> Serializer<'b> {
        self.integers.clear();
        self.booleans.clear();
        Serializer {
            integers: self.integers,
            strings: reuse_vec::<&'a str, &'b str>(self.strings),
            booleans: self.booleans,
        }
    }

    pub fn write_int(&mut self, value: i64) {
        self.integers.push(value);
    }

    pub fn write_string(&mut self, value: &'a str) {
        self.strings.push(value);
    }

    pub fn write_bool(&mut self, value: bool) {
        self.booleans.push(value);
    }

    pub fn finish(&self, buffer: &mut Vec<u8>) {
        let mut packer = BitPacker::new(buffer);

        packer.write_int(self.integers.len() as i64);
        packer.write_int(self.booleans.len() as i64);

        for integer in &self.integers {
            packer.write_int(*integer);
        }

        for boolean in &self.booleans {
            packer.write_bit(*boolean);
        }

        // Batch compress all strings together
        packer.write_strings(&self.strings);

        let native = self.native_bytes();
        let buffer = buffer.len();
        eprintln!(
            "buffer: {:?}, native: {:?}, compression: {:?}",
            buffer,
            native,
            buffer as f32 / native as f32
        );
    }

    pub fn native_bytes(&self) -> usize {
        std::mem::size_of::<bool>() * self.booleans.len()
            + std::mem::size_of::<i64>() * self.integers.len()
            + self.strings.iter().map(|s| s.len()).sum::<usize>()
    }
}

#[derive(Debug)]
pub struct Deserializer {
    // buffers
    integers: VecDeque<i64>,
    strings: VecDeque<String>,
    booleans: VecDeque<bool>,
}

impl Deserializer {
    pub fn new() -> Self {
        Self {
            integers: Default::default(),
            strings: Default::default(),
            booleans: Default::default(),
        }
    }

    pub fn read_bytes(&mut self, bytes: &[u8]) {
        let mut unpacker = BitUnpacker::new(bytes);

        let int_len = unpacker.read_int();
        let bool_len = unpacker.read_int();

        for _ in 0..int_len {
            self.integers.push_back(unpacker.read_int());
        }

        for _ in 0..bool_len {
            self.booleans.push_back(unpacker.read_bit());
        }

        // Read batch-compressed strings
        for s in unpacker.read_strings() {
            self.strings.push_back(s);
        }
    }

    pub fn take_int(&mut self) -> Option<i64> {
        self.integers.pop_front()
    }

    pub fn take_bool(&mut self) -> Option<bool> {
        self.booleans.pop_front()
    }

    pub fn take_string(&mut self) -> Option<String> {
        self.strings.pop_front()
    }
}

pub trait IntoFormat {
    fn serialize<'a>(&'a self, serializer: &mut Serializer<'a>);
    fn deserialize(data: &[u8], deserializer: &mut Deserializer) -> Option<Self>
    where
        Self: Sized;
}
