use crate::bit_packer::BitPacker;

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

// funny ownership hack into making the compiler re-use the allocated Vec
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

        /*
        for integer in &self.integers {
            // header
            let bytes = if *integer < i8::MAX as i64 {
                1
            } else if *integer < i16::MAX as i64 {
                2
            } else if *integer < i32::MAX as i64 {
                3
            } else {
                4
            };
            packer.write_bytes(*integer);
        }

        for boolean in &self.booleans {
            // simple bitset
            packer.write_bit(*boolean);
        }

        for string in &self.strings {
            packer.write_bytes(string.bytes());
            packer.write_bytes(std::iter::once(0)); // null terminated
        }
        */
    }
}

pub struct Deserializer {
    integer_index: usize,
    string_index: usize,
    boolean_index: usize,

    // buffers
    integers: Vec<i64>,
    strings: Vec<String>,
    booleans: Vec<bool>,
}

impl Deserializer {
    pub fn new() -> Self {
        Self {
            integer_index: 0,
            string_index: 0,
            boolean_index: 0,

            integers: Vec::new(),
            strings: Vec::new(),
            booleans: Vec::new(),
        }
    }
}

pub trait IntoFormat {
    #[must_use]
    fn serialize<'a, 'b>(&'b self, serializer: Serializer<'a>) -> Serializer<'a>;
    fn deserialize(data: &[u8], deserializer: &mut Deserializer) -> Self;
}
