mod bit_packer;
mod serializer;

use serializer::{Deserializer, IntoFormat, PropertyValue, Serializer};

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
        //     serializer.write_int(self.data);
        //     serializer.write_string(self.name.as_str());
        //     // serializer.write_bool(self.cool);
        // }
    }

    fn deserialize(data: &[u8], deserializer: &mut Deserializer<Self>) -> Option<Self> {
        deserializer.read_bytes(data);

        eprintln!("deser: {:?}", deserializer);

        Some(Config {
            data: deserializer.take_int()?,
            name: deserializer.take_string()?,
            cool: deserializer.take_bool()?,
            arr: deserializer.take_array()?,
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
        arr: vec![
            PropertyValue::String("testing testing".to_owned()),
            PropertyValue::Integer(500),
            PropertyValue::Bool(true),
            PropertyValue::Bool(false),
            PropertyValue::Bool(false),
            PropertyValue::Array(vec![
                PropertyValue::String("testing testing".to_owned()),
                PropertyValue::Integer(500),
                PropertyValue::Bool(true),
                PropertyValue::Bool(false),
                PropertyValue::Bool(false),
            ]),
        ],
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
