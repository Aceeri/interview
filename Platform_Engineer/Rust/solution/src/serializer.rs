use std::{borrow::Cow, collections::VecDeque, marker::PhantomData};

use crate::bit_packer::{BitPacker, BitUnpacker};

// Assumptions/requirements about schemas:
// - Support small configs primarily, but scale ok to larger ones if needed.
// - Strings are largely english & alphanumeric, but full unicode should still be supported.
// - Source of data is verified outside of serializing/deserializing, but invalid properties should still
//   be caught in case of any form of data corruption.
// - Versioning of schemas is handled by the user/tooling.
// - Schemas are compressed after serialization via mature libraries such as zstd/lz4, preprocessing and
//   entropy encoding are left out of the format to prevent interference with these more mature libraries,
//   instead focus mainly on reducing the amount of pointless entropy in our data.
#[derive(Debug, Default)]
pub struct Serializer<'a, S: IntoFormat> {
    // each property is order-dependent, arrays are flattened into this structure and theoretically
    // nested structs would do the same.

    // Keep similar types next to each other for 2 reasons:
    // - Based on some prior work on vertex compression, I found that grouping data together can get you
    //   closer to theoretical entropy limits.
    // - Compressors generally also like homogenous data nearby, improves pre-processing steps which then
    //   improves overall compression.

    // Main assumption: these are likely to be very small integers and fairly similar to eachother
    //
    // 3 bit bit length header with concentration around smaller numbers seems good, I don't think 4 bits
    // is really worth it since then your header is likely largely than the data itself, but I could be wrong.
    //
    // delta encoding?
    // daniel lemire's FastPFOR or similar would be worthwhile if we weren't expecting small amounts of properties.
    integers: Vec<i64>,
    // UTF-8 is fairly compact already, just write that to the buffer. Delta encoding might be the worthwhile here
    // too for compression assuming its mostly alphanumeric.
    //
    // null terminating the strings is a bit dangerous, but saves us a bit on bits. If we used the same format as
    // integers for the string length, then we only save on bits up to 16 bytes in, which seems unlikely.
    //
    // technically if just ascii is enough, then we could compact to 7 bits easily
    // 1-32 are also unused which means we are still only using 75% of the values, so this could be compressed
    // further if we are given a sequence of characters
    strings: Vec<Cow<'a, str>>,
    // booleans can just be bitpacked directly, meets shannon entropy theoretical limit directly
    booleans: Vec<bool>,
    // arrays can be dynamically typed and sized and nested
    //
    // length prefixed and an enum of each property inside of it.
    //
    // 2 bits per tag
    property_types: Vec<PropertyType>,

    marker: PhantomData<S>,
}

#[derive(Copy, Clone, Debug)]
pub enum PropertyType {
    String,
    Bool,
    Integer,
    Array,
}

impl PropertyType {
    pub fn to_bits(&self) -> (u8, u8) {
        match self {
            PropertyType::String => (0, 2),
            PropertyType::Bool => (1, 2),
            PropertyType::Integer => (2, 2),
            PropertyType::Array => (3, 2),
        }
    }

    pub fn from_bits(bits: u8) -> Option<Self> {
        match bits {
            0 => Some(PropertyType::String),
            1 => Some(PropertyType::Bool),
            2 => Some(PropertyType::Integer),
            3 => Some(PropertyType::Array),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum PropertyValue {
    String(String),
    Bool(bool),
    Integer(i64),
    Array(Vec<PropertyValue>),
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

impl<'a, S: IntoFormat> Serializer<'a, S> {
    pub fn new() -> Self {
        Self {
            integers: Vec::new(),
            strings: Vec::new(),
            booleans: Vec::new(),
            property_types: Vec::new(),
            marker: PhantomData,
        }
    }

    pub fn clear(&mut self) {
        self.integers.clear();
        self.strings.clear();
        self.booleans.clear();
        self.property_types.clear();
    }

    // for buffer re-use
    pub fn reuse<'b>(mut self) -> Serializer<'b, S> {
        self.integers.clear();
        self.booleans.clear();
        self.property_types.clear();
        Serializer {
            integers: self.integers,
            strings: reuse_vec(self.strings),
            booleans: self.booleans,
            property_types: self.property_types,
            marker: PhantomData,
        }
    }

    pub fn write_int(&mut self, value: i64) {
        self.integers.push(value);
    }

    pub fn write_string<'b: 'a>(&mut self, value: &'b str) {
        self.strings.push(Cow::Borrowed(value));
    }

    pub fn write_bool(&mut self, value: bool) {
        self.booleans.push(value);
    }

    pub fn write_value<'r: 'a>(&mut self, value: &'r PropertyValue) {
        match value {
            PropertyValue::Bool(bool) => {
                self.write_property_type(PropertyType::Bool);
                self.write_bool(*bool);
            }
            PropertyValue::String(string) => {
                self.write_property_type(PropertyType::String);
                self.write_string(string.as_str());
            }
            PropertyValue::Integer(int) => {
                self.write_property_type(PropertyType::Integer);
                self.write_int(*int);
            }
            PropertyValue::Array(values) => {
                self.write_property_type(PropertyType::Array);
                self.write_array(values.as_slice());
            }
        }
    }

    pub fn write_property_type(&mut self, tag: PropertyType) {
        self.property_types.push(tag);
    }

    pub fn write_array<'arr: 'a>(&mut self, array: &'arr [PropertyValue]) {
        self.write_int(array.len() as i64);
        for value in array {
            self.write_value(value);
        }
    }

    pub fn finish(&self, buffer: &mut Vec<u8>) {
        let mut packer = BitPacker::new(buffer);
        packer.write_byte(S::version());
        packer.write_int(self.integers.len() as i64);
        packer.write_int(self.booleans.len() as i64);
        packer.write_int(self.strings.len() as i64); // maybe unnecessary?
        packer.write_int(self.property_types.len() as i64);

        println!(
            "serialize lens: {:?} {:?} {:?} {:?}",
            self.integers.len(),
            self.booleans.len(),
            self.strings.len(),
            self.property_types.len()
        );

        for integer in &self.integers {
            packer.write_int(*integer);
        }

        for boolean in &self.booleans {
            packer.write_bit(*boolean);
        }

        for string in &self.strings {
            packer.write_string(string);
        }

        for tag in &self.property_types {
            packer.write_property_type(*tag);
        }

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
            + std::mem::size_of::<PropertyType>() * self.property_types.len()
    }
}

#[derive(Debug)]
pub struct Deserializer<S: IntoFormat> {
    // buffers
    integers: VecDeque<i64>,
    strings: VecDeque<String>,
    booleans: VecDeque<bool>,
    property_types: VecDeque<PropertyType>,

    marker: PhantomData<S>,
}

impl<S: IntoFormat> Deserializer<S> {
    pub fn new() -> Self {
        Self {
            integers: Default::default(),
            strings: Default::default(),
            booleans: Default::default(),
            property_types: Default::default(),

            marker: PhantomData,
        }
    }

    // should ideally a `Result`
    pub fn read_bytes(&mut self, bytes: &[u8]) -> Option<()> {
        let mut unpacker = BitUnpacker::new(bytes);

        let version = unpacker.read_byte()?;
        assert_eq!(version, S::version());

        println!("version: {:?}", version);

        let int_len = unpacker.read_int()?;
        let bool_len = unpacker.read_int()?;
        let string_len = unpacker.read_int()?;
        let tags_len = unpacker.read_int()?;
        println!(
            "lens: {:?} {:?} {:?} {:?}",
            int_len, bool_len, string_len, tags_len
        );

        for _ in 0..int_len {
            self.integers.push_back(unpacker.read_int()?);
        }

        for _ in 0..bool_len {
            self.booleans.push_back(unpacker.read_bit()?);
        }

        for _ in 0..string_len {
            self.strings.push_back(unpacker.read_string()?);
        }

        for _ in 0..tags_len {
            self.property_types
                .push_back(unpacker.read_property_type()?);
        }

        Some(())
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

    pub fn take_property_type(&mut self) -> Option<PropertyType> {
        self.property_types.pop_front()
    }

    pub fn take_array(&mut self) -> Option<Vec<PropertyValue>> {
        // we take advantage of integer compression here, because arrays are likely less than 16 elements.
        let length = self.take_int()? as usize;

        let mut values = Vec::with_capacity(length);
        for _ in 0..length {
            let tag = self.take_property_type()?;
            println!("tag: {:?}", tag);

            let value = match tag {
                PropertyType::String => PropertyValue::String(self.take_string()?),
                PropertyType::Bool => PropertyValue::Bool(self.take_bool()?),
                PropertyType::Integer => PropertyValue::Integer(self.take_int()?),
                PropertyType::Array => PropertyValue::Array(self.take_array()?),
            };
            values.push(value);
        }

        Some(values)
    }
}

pub trait IntoFormat {
    fn version() -> u8;
    fn serialize<'a>(&'a self, serializer: &mut Serializer<'a, Self>)
    where
        Self: Sized;
    fn deserialize(data: &[u8], deserializer: &mut Deserializer<Self>) -> Option<Self>
    where
        Self: Sized;
}
