mod bit_packer;
mod huffman;
mod serializer;

use serializer::{Deserializer, IntoFormat, Serializer};

#[derive(Debug)]
pub struct Config {
    data: i64,
    name: String,
    cool: bool,
}

impl IntoFormat for Config {
    fn serialize<'a>(&'a self, serializer: &mut Serializer<'a>) {
        serializer.write_int(self.data);
        serializer.write_string(self.name.as_str());
        serializer.write_bool(self.cool);

        // for _ in 0..200 {
        //     serializer.write_int(self.data);
        //     serializer.write_string(self.name.as_str());
        //     serializer.write_bool(self.cool);
        // }
    }

    fn deserialize(data: &[u8], deserializer: &mut Deserializer) -> Option<Self> {
        deserializer.read_bytes(data);

        eprintln!("deser: {:?}", deserializer);

        Some(Config {
            data: deserializer.take_int()?,
            name: deserializer.take_string()?,
            cool: deserializer.take_bool()?,
        })
    }
}

fn main() {
    let mut serializer = Serializer::new();
    let mut deserializer = Deserializer::new();

    // let taken = std::mem::take(serializer); // `Vec::new` does not allocate, so we temporarily take this
    // let mut struct_ser = taken.reuse(); // change the lifetime on buffers

    let config = Config {
        data: 4,
        name: "Nice".to_owned(),
        cool: true,
    };

    let mut buffer = Vec::new();
    config.serialize(&mut serializer);
    serializer.finish(&mut buffer);

    // *serializer = struct_ser.reuse(); // return the lifetime

    println!("buffer: {:?}", buffer);
    let new_config = Config::deserialize(&buffer, &mut deserializer);
    println!("original: {:?}", config);
    println!("deserialized: {:?}", new_config);
}
