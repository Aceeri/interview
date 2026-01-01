mod bit_packer;
mod serializer;
mod ultra_packer;

use serializer::{Deserializer, IntoFormat, PropertyValue, Serializer};

use crate::bit_packer::{ASCII_BITS_PER_BUNDLE, ASCII_BUNDLE_SIZE};

#[derive(Debug, PartialEq, Eq)]
pub struct Config {
    data: i64,
    name: String,
    cool: bool,
    arr: Vec<PropertyValue>,
    other: OtherConfig,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OtherConfig {
    nested: i64,
}

impl IntoFormat for OtherConfig {
    fn serialize<'a>(&'a self, serializer: &mut Serializer<'a>) {
        serializer.write_int(self.nested);
    }

    fn take(deserializer: &mut Deserializer) -> Option<Self> {
        Some(OtherConfig {
            nested: deserializer.take_int()?,
        })
    }
}

impl IntoFormat for Config {
    fn serialize<'a>(&'a self, serializer: &mut Serializer<'a>) {
        serializer.write_int(self.data);
        serializer.write_string(self.name.as_str());
        serializer.write_bool(self.cool);
        serializer.write_array(self.arr.as_slice());
        self.other.serialize(serializer);
    }

    fn take(deserializer: &mut Deserializer) -> Option<Self> {
        Some(Config {
            data: deserializer.take_int()?,
            name: deserializer.take_string()?,
            cool: deserializer.take_bool()?,
            arr: deserializer.take_array()?,
            other: OtherConfig::take(deserializer)?,
        })
    }
}

fn main() {
    let mut serializer = Serializer::new();
    let mut deserializer = Deserializer::new();

    let config = Config {
        data: 4,
        name: "Nice".to_owned(),
        cool: true,
        other: OtherConfig { nested: 5481 },
        arr: vec![
            PropertyValue::String("46 KiB".to_owned()),
            PropertyValue::String("falling 1928".to_owned()),
            PropertyValue::String("1920x1080".to_owned()),
            PropertyValue::String("0.588293, 9182.382".to_owned()),
            PropertyValue::String("/usr/local/bin/test".to_owned()),
            PropertyValue::String("entry.sh".to_owned()),
            PropertyValue::String("Canon EOS 90D".to_owned()),
            PropertyValue::String("Canon".to_owned()),
            PropertyValue::Integer(500),
            PropertyValue::Integer(256),
            PropertyValue::Integer(4096),
            PropertyValue::Integer(18273),
            PropertyValue::Integer(18273),
            PropertyValue::Integer(4),
            PropertyValue::Integer(4),
            PropertyValue::Integer(4),
            PropertyValue::Integer(1),
            PropertyValue::Integer(1),
            PropertyValue::Integer(31415926535897),
            PropertyValue::Integer(5),
            PropertyValue::Integer(50),
            PropertyValue::Integer(64),
            PropertyValue::Integer(128),
            PropertyValue::Integer(100),
            PropertyValue::Integer(100),
            PropertyValue::Integer(100),
            PropertyValue::Integer(9999999),
            PropertyValue::Bool(true),
            PropertyValue::Bool(true),
            PropertyValue::Bool(true),
            PropertyValue::Bool(false),
            PropertyValue::Bool(false),
            PropertyValue::Bool(false),
            PropertyValue::Bool(true),
            PropertyValue::Bool(false),
            PropertyValue::Bool(true),
            PropertyValue::Bool(true),
            PropertyValue::Bool(true),
            PropertyValue::Bool(true),
            PropertyValue::Bool(true),
            PropertyValue::Bool(false),
            PropertyValue::Bool(false),
            PropertyValue::Array(vec![
                PropertyValue::String("testing".to_owned()),
                PropertyValue::Integer(500),
                PropertyValue::Bool(true),
                PropertyValue::Bool(false),
                PropertyValue::Bool(false),
            ]),
            PropertyValue::String("46 KiB".to_owned()),
            PropertyValue::String("2021:09:17 13:26:08+02:00".to_owned()),
            PropertyValue::String("2021:09:20 10:12:27+02:00".to_owned()),
            PropertyValue::String("2021:09:17 13:51:44+02:00".to_owned()),
            PropertyValue::String("JPEG".to_owned()),
            PropertyValue::String("jpg".to_owned()),
            PropertyValue::String("image/jpeg".to_owned()),
            PropertyValue::String("Little-endian (Intel, II)".to_owned()),
            PropertyValue::String("Nikon".to_owned()),
            PropertyValue::String("p1000".to_owned()),
            PropertyValue::String("1/8000".to_owned()),
            PropertyValue::String("4.0".to_owned()),
            PropertyValue::String("500".to_owned()),
            PropertyValue::String("2021:06:11 21:20:48".to_owned()),
            PropertyValue::String("2021:06:11 21:20:48".to_owned()),
            PropertyValue::String("3.8 mm".to_owned()),
            PropertyValue::String("0 mm".to_owned()),
            PropertyValue::String("289.8 m".to_owned()),
            PropertyValue::String("XMP Core 4.4.0-Exiv2".to_owned()),
            // PropertyValue::String(
            //     "[minor] Fixed incorrect URI for xmlns:MicrosoftPhoto".to_owned(),
            // ),
            // PropertyValue::String("1254-9561".to_owned()),
            // PropertyValue::String("1254-9561".to_owned()),
            // PropertyValue::String("".to_owned()),
            // PropertyValue::String("320".to_owned()),
            // PropertyValue::String("477".to_owned()),
            // PropertyValue::String("Baseline DCT, Huffman coding".to_owned()),
            // PropertyValue::String("8".to_owned()),
            // PropertyValue::String("3".to_owned()),
            // PropertyValue::String("YCbCr4:2:0 (2 2)".to_owned()),
            // PropertyValue::String("4.0".to_owned()),
            // PropertyValue::String("320x477".to_owned()),
            // PropertyValue::String("0.153".to_owned()),
            // PropertyValue::String("1/8000".to_owned()),
            // PropertyValue::String("52 deg 14' 25.90\" N".to_owned()),
            // PropertyValue::String("21 deg 0' 59.59\" E".to_owned()),
            // PropertyValue::String("North".to_owned()),
            // PropertyValue::String("East".to_owned()),
            // PropertyValue::String("3.8 mm".to_owned()),
            // PropertyValue::String("52 deg 14' 25.90\" N, 21 deg 0' 59.59\" E".to_owned()),
            // PropertyValue::String("14.6".to_owned()),
        ],
    };

    const PROTOCOL_VERSION: u8 = 0u8;

    let mut buffer = Vec::new();
    let mut native_buffer = Vec::new();
    config.serialize(&mut serializer);
    serializer.finish(&mut buffer, PROTOCOL_VERSION);
    serializer.finish_native(&mut native_buffer, PROTOCOL_VERSION);

    println!("native_buffer: {:?}", native_buffer.len());
    println!("buffer: {:?}", buffer.len());
    println!(
        "format compression ratio: {:.1}%",
        (1.0 - buffer.len() as f64 / native_buffer.len() as f64) * 100.0
    );

    let new_config = Config::deserialize(&buffer, &mut deserializer, PROTOCOL_VERSION);
    println!("original: {:?}", config);
    println!("deserialized: {:?}", new_config);
    println!("same?: {:?}", Some(config) == new_config);

    // Testing to see how zstd fairs vs bitpacking
    //
    // Use native buffer instead of bitpacked, since the bitpacking can cause issues with entropy encoding.
    //
    // Compress with zstd
    let compressed = zstd::bulk::compress(&native_buffer, 3).expect("zstd compression failed");
    println!("native buffer: {} bytes", native_buffer.len());
    println!("zstd compressed: {} bytes", compressed.len());
    println!(
        "zstd compression ratio: {:.1}%",
        (1.0 - compressed.len() as f64 / native_buffer.len() as f64) * 100.0
    );
}
