use byteorder::{BigEndian, ReadBytesExt};
use std::io::prelude::*;

mod decompnode;
use decompnode::DecompNode;

fn wrap256(n: u8) -> usize {
    if n == 0 {
        256
    } else {
        n as usize
    }
}

pub struct DLDecoder {
    gfxram: Vec<u8>,
    decomp_table: [[DecompNode; 2]; 512],
}

impl Default for DLDecoder {
    fn default() -> Self {
        Self {
            gfxram: vec![0u8; 0x100_0000],
            decomp_table: [[DecompNode { color: 0, next: 0 }; 2]; 512],
        }
    }
}

impl DLDecoder {
    pub fn dumpbuffer(&self, buf: &mut [u8], addr: usize, len: usize) {
        buf.copy_from_slice(&self.gfxram[addr..addr + len]);
    }

    pub fn load_decomp(&mut self, reader: &mut dyn BufRead) {
        reader.consume(4);
        let cnt = reader.read_u32::<BigEndian>().unwrap();
        let mut nodebuf = [0u8; 9];
        for i in 0..cnt {
            reader.read(&mut nodebuf).unwrap();
            self.decomp_table[i as usize] = DecompNode::read_from(&nodebuf);
        }
    }

    pub fn memcopy8(&mut self, reader: &mut dyn BufRead) {
        let dstaddr = reader.read_u24::<BigEndian>().unwrap() as usize;
        let cnt = wrap256(reader.read_u8().unwrap());
        let srcaddr = reader.read_u24::<BigEndian>().unwrap() as usize;
        self.gfxram.copy_within(srcaddr..srcaddr + cnt, dstaddr);
    }

    pub fn memcopy16(&mut self, reader: &mut dyn BufRead) {
        let dstaddr = reader.read_u24::<BigEndian>().unwrap() as usize;
        let cnt = wrap256(reader.read_u8().unwrap()) * 2;
        let srcaddr = reader.read_u24::<BigEndian>().unwrap() as usize;
        self.gfxram.copy_within(srcaddr..srcaddr + cnt, dstaddr);
    }

    pub fn fill8(&mut self, reader: &mut dyn BufRead) {
        let mut addr = reader.read_u24::<BigEndian>().unwrap() as usize;
        let mut totalcnt = wrap256(reader.read_u8().unwrap());
        while totalcnt > 0 {
            let cnt = wrap256(reader.read_u8().unwrap());
            let value = reader.read_u8().unwrap();
            for i in 0..cnt {
                self.gfxram[addr + i] = value;
            }
            totalcnt -= cnt;
            addr += cnt;
        }
    }

    pub fn fill16(&mut self, reader: &mut dyn BufRead) {
        let mut addr = reader.read_u24::<BigEndian>().unwrap() as usize;
        let mut totalcnt = wrap256(reader.read_u8().unwrap());
        while totalcnt > 0 {
            let cnt = wrap256(reader.read_u8().unwrap());
            let mut value = [0u8; 2];
            reader.read_exact(&mut value).unwrap();
            for i in 0..cnt {
                self.gfxram[addr + i * 2] = value[1];
                self.gfxram[addr + i * 2 + 1] = value[0];
            }
            totalcnt -= cnt;
            addr += cnt * 2;
        }
    }

    pub fn decomp8(&mut self, reader: &mut dyn BufRead) {
        let addr = reader.read_u24::<BigEndian>().unwrap() as usize;
        let cnt = wrap256(reader.read_u8().unwrap());

        let mut tableidx = 0;
        let mut accumulator = 0u8;
        let mut bitcnt = 8;
        let mut bytebuf = 0;
        for i in 0..cnt {
            loop {
                if bitcnt == 8 {
                    bytebuf = reader.read_u8().unwrap();
                    bitcnt = 0;
                }
                let decomp_entry = &self.decomp_table[tableidx][(bytebuf & 1) as usize];
                bytebuf >>= 1;
                bitcnt += 1;
                accumulator = accumulator.wrapping_add((decomp_entry.color & 0xff) as u8);
                tableidx = decomp_entry.next;
                if tableidx == 0 {
                    break;
                }
            }
            self.gfxram[addr + i] = accumulator;
            tableidx = 0;
        }
    }

    pub fn decomp16(&mut self, reader: &mut dyn BufRead) {
        let addr = reader.read_u24::<BigEndian>().unwrap() as usize;
        let cnt = wrap256(reader.read_u8().unwrap());

        let mut tableidx = 8;
        let mut accumulator = 0u16;
        let mut bitcnt = 8;
        let mut bytebuf = 0;
        for i in 0..cnt {
            loop {
                if bitcnt == 8 {
                    bytebuf = reader.read_u8().unwrap();
                    bitcnt = 0;
                }
                let decomp_entry = &self.decomp_table[tableidx][(bytebuf & 1) as usize];
                bytebuf >>= 1;
                bitcnt += 1;
                accumulator = accumulator.wrapping_add(decomp_entry.color);
                tableidx = decomp_entry.next;
                if tableidx == 0 {
                    break;
                }
            }
            self.gfxram[addr + i * 2 + 1] = (accumulator >> 8) as u8;
            self.gfxram[addr + i * 2] = (accumulator & 0xff) as u8;
            tableidx = 8;
        }
    }
}
