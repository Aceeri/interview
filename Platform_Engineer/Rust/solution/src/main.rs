mod bit_packer;
mod serializer;

use serializer::{Deserializer, IntoFormat, Serializer};

pub struct Config {
    data: i64,
    name: String,
    cool: bool,
}

impl IntoFormat for Config {
    fn serialize<'a, 'b>(&'b self, mut serializer: Serializer<'a>) -> Serializer<'a> {
        let mut struct_ser = serializer.reuse();
        struct_ser.write_int(self.data);
        struct_ser.write_string(self.name.as_str());
        struct_ser.write_bool(self.cool);
        serializer = struct_ser.reuse();
        serializer
    }

    fn deserialize(data: &[u8], deserializer: &mut Deserializer) -> Self {
        Config {
            data: 4,
            name: "Nice".to_owned(),
            cool: true,
        }
    }
}

fn main() {
    let mut serializer = Serializer::new();

    let mut deserializer = Deserializer::new();

    let config = Config {
        data: 4,
        name: "Nice".to_owned(),
        cool: true,
    };

    let serializer = config.serialize(serializer);
}
