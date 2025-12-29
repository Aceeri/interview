pub struct BitPacker<'a> {
    buffer: &'a mut Vec<u8>,
    bit_offset: u8,
}

impl<'a> BitPacker<'a> {
    pub fn new(buffer: &'a mut Vec<u8>) -> Self {
        buffer.clear();
        BitPacker {
            buffer,
            bit_offset: 0,
        }
    }

    pub fn write_bytes(&mut self, bytes: impl Iterator<Item = u8>) {
        if self.bit_offset == 0 {
            // we are aligned, fast path
            self.buffer.extend(bytes);
        } else {
            for byte in bytes {
                // need to write to 2 different bytes, 1 existing, 1 new
                let left_mask = byte << (7 - self.bit_offset);
                let right_mask = byte >> self.bit_offset;
                let last = self.buffer.len() - 1;
                self.buffer[last] |= left_mask;
                self.buffer.push(right_mask);
            }
        }
    }

    pub fn write_bit(&mut self, bit: bool) {
        if self.bit_offset == 8 {
            self.buffer.push(0);
            self.bit_offset = 0;
        }

        if bit {
            let last = self.buffer.len() - 1;
            self.buffer[last] |= 1 << (7 - self.bit_offset);
        }

        self.bit_offset += 1;
    }

    pub fn finish(&self) -> Vec<u8> {
        self.buffer.clone()
    }
}

mod tests {
    use crate::bit_packer::BitPacker;

    #[test]
    pub fn sanity() {
        let mut buffer = Vec::new();
        let mut packer = BitPacker::new(&mut buffer);

        packer.write_bit(true);
        packer.write_bit(true);
        packer.write_bit(true);
        packer.write_bit(true);

        assert_eq!(buffer, vec![0b00001111]);
    }
}
