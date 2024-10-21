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

    pub fn load_decomp(&mut self, reader: &mut dyn BufRead) -> Result<(), std::io::Error> {
        reader.consume(4);
        let cnt = reader.read_u32::<BigEndian>()?;
        let mut nodebuf = [0u8; 9];
        for i in 0..cnt {
            reader.read_exact(&mut nodebuf)?;
            self.decomp_table[i as usize] = DecompNode::read_from(&nodebuf);
        }
        Ok(())
    }

    pub fn memcopy8(&mut self, reader: &mut dyn BufRead) -> Result<(), std::io::Error> {
        let dstaddr = reader.read_u24::<BigEndian>()? as usize;
        let cnt = wrap256(reader.read_u8()?);
        let srcaddr = reader.read_u24::<BigEndian>()? as usize;
        self.gfxram.copy_within(srcaddr..srcaddr + cnt, dstaddr);
        Ok(())
    }

    pub fn memcopy16(&mut self, reader: &mut dyn BufRead) -> Result<(), std::io::Error> {
        let dstaddr = reader.read_u24::<BigEndian>()? as usize;
        let cnt = wrap256(reader.read_u8()?) * 2;
        let srcaddr = reader.read_u24::<BigEndian>()? as usize;
        self.gfxram.copy_within(srcaddr..srcaddr + cnt, dstaddr);
        Ok(())
    }

    pub fn fill8(&mut self, reader: &mut dyn BufRead) -> Result<(), std::io::Error> {
        let mut addr = reader.read_u24::<BigEndian>()? as usize;
        let mut totalcnt = wrap256(reader.read_u8()?);
        while totalcnt > 0 {
            let cnt = wrap256(reader.read_u8()?);
            let value = reader.read_u8()?;
            for i in 0..cnt {
                self.gfxram[addr + i] = value;
            }
            totalcnt -= cnt;
            addr += cnt;
        }
        Ok(())
    }

    pub fn fill16(&mut self, reader: &mut dyn BufRead) -> Result<(), std::io::Error> {
        let mut addr = reader.read_u24::<BigEndian>()? as usize;
        let mut totalcnt = wrap256(reader.read_u8()?);
        while totalcnt > 0 {
            let cnt = wrap256(reader.read_u8()?);
            let mut value = [0u8; 2];
            reader.read_exact(&mut value)?;
            for i in 0..cnt {
                self.gfxram[addr + i * 2] = value[1];
                self.gfxram[addr + i * 2 + 1] = value[0];
            }
            totalcnt -= cnt;
            addr += cnt * 2;
        }
        Ok(())
    }

    pub fn decomp8(&mut self, reader: &mut dyn BufRead) -> Result<(), std::io::Error> {
        let addr = reader.read_u24::<BigEndian>()? as usize;
        let cnt = wrap256(reader.read_u8()?);

        let mut tableidx = 0;
        let mut accumulator = 0u8;
        let mut bitcnt = 8;
        let mut bytebuf = 0;
        for i in 0..cnt {
            loop {
                if bitcnt == 8 {
                    bytebuf = reader.read_u8()?;
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
        Ok(())
    }

    pub fn decomp16(&mut self, reader: &mut dyn BufRead) -> Result<(), std::io::Error> {
        let addr = reader.read_u24::<BigEndian>()? as usize;
        let cnt = wrap256(reader.read_u8()?);

        let mut tableidx = 8;
        let mut accumulator = 0u16;
        let mut bitcnt = 8;
        let mut bytebuf = 0;
        for i in 0..cnt {
            loop {
                if bitcnt == 8 {
                    bytebuf = reader.read_u8()?;
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
        Ok(())
    }
}
