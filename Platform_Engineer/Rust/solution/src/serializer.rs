use std::{borrow::Cow, collections::VecDeque, marker::PhantomData};

use crate::bit_packer::{BitPacker, BitUnpacker};

// Assumptions/requirements about schemas:
// - Primarily focus on small configs, avoid bloating beyond initial size.
// - Source of data is verified outside of serializing/deserializing, but invalid properties should still
//   be caught in case of any form of data corruption.
// - Version id assignment of schemas is handled by the user/tooling.

#[derive(Debug, Default)]
pub struct Serializer<'a> {
    // each property is order-dependent, arrays are flattened into this structure and theoretically
    // nested structs would do the same.

    // Keep similar types next to each other for 2 reasons:
    // - Based on some prior work on vertex compression, grouping data together can get you
    //   closer to theoretical entropy limits.
    // - Compressors generally also like homogenous data nearby, improves pre-processing steps which then
    //   improves overall compression.

    // Main assumption: these are likely to be very small integers and fairly similar to eachother
    //
    // 3 bit bit length header with concentration around smaller numbers seems good, I don't think 4 bits
    // is really worth it since then your header is likely largely than the data itself.
    //
    // delta encoding?
    // daniel lemire's FastPFOR or similar would be worthwhile if we were expecting larger amounts of integers.
    integers: Vec<i64>,
    // UTF-8 is fairly compact already, just write that to the buffer if need be.
    //
    // null terminating the strings is a bit dangerous, but even if we used the same format as
    // integers for the string length, then we only save on bits up to 16 bytes in, which seems too close to
    // a median for probable values.
    //
    // If all strings are ascii, then we could compact to 7 bits easily.
    // 1-32 are also unused which means we are still only using 75% of the values, so this could be compressed
    // further if we are given a sequence of characters
    //
    // Lets try using a single bit header of "all-ascii", then we can encode the common path better.
    strings: Vec<Cow<'a, str>>,
    // booleans can just be bitpacked directly, RLE *may* help sometimes, but given mostly random booleans
    // it'll just bloat this size.
    booleans: Vec<bool>,
    // arrays can be dynamically typed and sized and nested
    //
    // length prefixed and an enum of each property inside of it.
    //
    // 2 bits per tag
    property_types: Vec<PropertyType>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropertyValue {
    String(String),
    Bool(bool),
    Integer(i64),
    Array(Vec<PropertyValue>),
}

// hacky way to get the compiler to re-use the allocated Vec for differing lifetimes
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
            property_types: Vec::new(),
        }
    }

    // should generally hint to the compiler enough that we can re-use this serializer.
    pub fn reuse<'b>(mut self) -> Serializer<'b> {
        self.integers.clear();
        self.booleans.clear();
        self.property_types.clear();
        Serializer {
            integers: self.integers,
            strings: reuse_vec(self.strings),
            booleans: self.booleans,
            property_types: self.property_types,
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

    // are we ascii & are we above the "control" characters?
    // if so we can save at least
    pub fn all_32_127(&self) -> bool {
        self.strings
            .iter()
            .any(|string| string.chars().any(|c| c as u32 >= 32 && c as u32 <= 127))
    }

    pub fn finish_native(&self, buffer: &mut Vec<u8>, version: u8) {
        let mut packer = BitPacker::new(buffer);
        packer.write_byte(version);
        packer.write_bytes(&(self.integers.len() as i64).to_le_bytes());
        packer.write_bytes(&(self.booleans.len() as i64).to_le_bytes());
        packer.write_bytes(&(self.strings.len() as i64).to_le_bytes());
        packer.write_bytes(&(self.property_types.len() as i64).to_le_bytes());

        for integer in &self.integers {
            packer.write_bytes(&integer.to_le_bytes());
        }

        for string in &self.strings {
            packer.write_bytes(string.as_bytes());
        }

        for boolean in &self.booleans {
            packer.write_bytes(&[*boolean as u8]);
        }

        for tag in &self.property_types {
            let (byte, _) = tag.to_bits();
            packer.write_bytes(&[byte]);
        }
    }

    pub fn finish(&self, buffer: &mut Vec<u8>, version: u8) {
        let mut packer = BitPacker::new(buffer);
        packer.write_byte(version);

        // per type headers
        packer.write_int(self.integers.len() as i64);
        packer.write_int(self.booleans.len() as i64);
        let all_ascii = self.all_32_127();
        // let all_ascii = false;
        packer.write_bit(all_ascii);
        packer.write_int(self.strings.len() as i64);
        packer.write_int(self.property_types.len() as i64);

        for integer in &self.integers {
            packer.write_int(*integer);
        }

        for boolean in &self.booleans {
            packer.write_bit(*boolean);
        }

        for string in &self.strings {
            if all_ascii {
                // packer.write_ascii_string(string);
                // packer.write_ascii_string_ultrapacked(string);
                packer.write_ascii_string_charset(string);
            } else {
                // need to encode as utf-8 directly
                packer.write_string(string);
            }
        }

        for tag in &self.property_types {
            packer.write_property_type(*tag);
        }
    }
}

#[derive(Debug)]
pub struct Deserializer {
    // buffers
    integers: VecDeque<i64>,
    strings: VecDeque<String>,
    booleans: VecDeque<bool>,
    property_types: VecDeque<PropertyType>,
}

impl Deserializer {
    pub fn new() -> Self {
        Self {
            integers: Default::default(),
            strings: Default::default(),
            booleans: Default::default(),
            property_types: Default::default(),
        }
    }

    // should ideally a `Result`
    pub fn read_bytes(&mut self, bytes: &[u8], version: u8) -> Option<()> {
        let mut unpacker = BitUnpacker::new(bytes);

        let read_version = unpacker.read_byte()?;
        assert_eq!(read_version, version);

        let int_len = unpacker.read_int()?;
        let bool_len = unpacker.read_int()?;

        let all_ascii = unpacker.read_bit()?;
        let string_len = unpacker.read_int()?;

        let tags_len = unpacker.read_int()?;

        for _ in 0..int_len {
            self.integers.push_back(unpacker.read_int()?);
        }

        for _ in 0..bool_len {
            self.booleans.push_back(unpacker.read_bit()?);
        }

        if all_ascii {
            for _ in 0..string_len {
                // self.strings.push_back(unpacker.read_ascii_string()?);
                self.strings
                    .push_back(unpacker.read_ascii_string_ultrapacked()?);
            }
        } else {
            for _ in 0..string_len {
                self.strings.push_back(unpacker.read_string()?);
            }
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
    fn serialize<'a>(&'a self, serializer: &mut Serializer<'a>)
    where
        Self: Sized;
    fn take(deserializer: &mut Deserializer) -> Option<Self>
    where
        Self: Sized;
    fn deserialize(data: &[u8], deserializer: &mut Deserializer, version: u8) -> Option<Self>
    where
        Self: Sized,
    {
        deserializer.read_bytes(data, version)?;
        Self::take(deserializer)
    }
}
