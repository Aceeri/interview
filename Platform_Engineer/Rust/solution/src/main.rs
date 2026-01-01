mod bit_packer;
mod serializer;
mod ultra_packer;

use serializer::{Deserializer, IntoFormat, PropertyValue, Serializer};

use crate::bit_packer::{ASCII_BITS_PER_BUNDLE, ASCII_BUNDLE_SIZE};

#[derive(Debug)]
pub struct Config {
    data: i64,
    name: String,
    cool: bool,
    arr: Vec<PropertyValue>,
}

impl IntoFormat for Config {
    fn version() -> u8 {
        8u8
    }

    fn serialize<'a>(&'a self, serializer: &mut Serializer<'a, Self>) {
        serializer.write_int(self.data);
        serializer.write_string(self.name.as_str());
        serializer.write_bool(self.cool);
        serializer.write_array(self.arr.as_slice());

        // for _ in 0..200 {
        //     // serializer.write_int(self.data);
        //     serializer.write_string("following the long path");
        //     serializer.write_string("Canon");
        //     serializer.write_string("Canon EOS 95D");
        //     serializer.write_string("1920x1080");
        //     serializer.write_string("600");
        //     // serializer.write_bool(self.cool);
        // }
    }

    fn deserialize(data: &[u8], deserializer: &mut Deserializer<Self>) -> Option<Self> {
        deserializer.read_bytes(data);

        eprintln!("deser: {:?}", deserializer);

        Some(Config {
            data: 0,
            name: deserializer.take_string()?,
            cool: false,
            arr: Vec::default(),
            // data: deserializer.take_int()?,
            // name: deserializer.take_string()?,
            // cool: deserializer.take_bool()?,
            // arr: deserializer.take_array()?,
        })
    }
}

fn main() {
    println!("{:?} {:?}", ASCII_BUNDLE_SIZE, ASCII_BITS_PER_BUNDLE);
    let mut serializer = Serializer::new();
    let mut deserializer = Deserializer::new();

    // let taken = std::mem::take(serializer); // `Vec::new` does not allocate, so we temporarily take this
    // let mut struct_ser = taken.reuse(); // change the lifetime on buffers

    let config = Config {
        data: 4,
        name: "Nice".to_owned(),
        cool: true,
        arr: vec![
            PropertyValue::String("testing 1928".to_owned()),
            PropertyValue::String("1920x1080".to_owned()),
            PropertyValue::String("0.588293, 9182.382".to_owned()),
            PropertyValue::String("/usr/local/bin/test".to_owned()),
            PropertyValue::String("/usr/local/bin/entry.sh".to_owned()),
            PropertyValue::String("Canon EOS 90D".to_owned()),
            PropertyValue::String("Canon".to_owned()),
            // PropertyValue::Integer(500),
            // PropertyValue::Bool(true),
            // PropertyValue::Bool(false),
            // PropertyValue::Bool(false),
            PropertyValue::Array(vec![
                PropertyValue::String("testing testing".to_owned()),
                // PropertyValue::Integer(500),
                // PropertyValue::Bool(true),
                // PropertyValue::Bool(false),
                // PropertyValue::Bool(false),
            ]),
        ],
    };

    let mut buffer = Vec::new();
    config.serialize(&mut serializer);
    serializer.finish(&mut buffer);

    let new_config = Config::deserialize(&buffer, &mut deserializer);
    println!("original: {:?}", config);
    println!("deserialized: {:?}", new_config);

    // Test to see how zstd fairs vs ultrapacking
    // Compress with zstd
    let compressed = zstd::bulk::compress(&buffer, 3).expect("zstd compression failed");

    // Decompress for deserialization
    let decompressed =
        zstd::bulk::decompress(&compressed, buffer.len() * 2).expect("zstd decompression failed");

    println!("raw buffer: {} bytes", buffer.len());
    println!("zstd compressed: {} bytes", compressed.len());
    println!(
        "zstd compression ratio: {:.1}%",
        (1.0 - compressed.len() as f64 / buffer.len() as f64) * 100.0
    );
}
