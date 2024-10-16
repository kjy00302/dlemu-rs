use byteorder::{BigEndian, ByteOrder};

#[derive(Clone, Copy, Debug)]
pub struct DecompNode {
    pub color: u16,
    pub next: usize,
}

impl DecompNode {
    pub fn read_from(buf: &[u8; 9]) -> [DecompNode; 2] {
        let color_a = BigEndian::read_u16(&buf[0..2]);
        let _a = buf[2];
        let a = buf[3];
        let ab = buf[4];
        let color_b = BigEndian::read_u16(&buf[5..7]);
        let _b = buf[7];
        let b = buf[8];

        [
            DecompNode {
                color: color_a,
                next: ((a & 0x1f) as usize) << 4 | (ab >> 4) as usize,
            },
            DecompNode {
                color: color_b,
                next: ((b & 0x1f) as usize) << 4 | (ab & 0xf) as usize,
            },
        ]
    }
}
