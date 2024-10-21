use byteorder::{BigEndian, ByteOrder, ReadBytesExt};
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

pub enum DLDecoderResult {
    FILL(usize, usize),
    MEMCPY(usize, usize),
    DECOMP(usize, usize),
    SETREG(u8, u8),
    NOOP,
}

pub struct DLDecoder {
    gfxram: Vec<u8>,
    reg: [u8; 256],
    decomp_table: [[DecompNode; 2]; 512],
}

impl Default for DLDecoder {
    fn default() -> Self {
        Self {
            gfxram: vec![0u8; 0x100_0000],
            reg: [0u8; 256],
            decomp_table: [[DecompNode { color: 0, next: 0 }; 2]; 512],
        }
    }
}

impl DLDecoder {
    pub fn dumpbuffer(&self, buf: &mut [u8], addr: usize, len: usize) {
        buf.copy_from_slice(&self.gfxram[addr..addr + len]);
    }

    pub fn set_reg(&mut self, addr: u8, value: u8) {
        self.reg[addr as usize] = value
    }
    pub fn get_reg(&mut self, addr: u8) -> u8 {
        self.reg[addr as usize]
    }
    pub fn get_width(&self) -> usize {
        BigEndian::read_u16(&self.reg[0x0f..0x11]) as usize
    }
    pub fn get_height(&self) -> usize {
        BigEndian::read_u16(&self.reg[0x17..0x19]) as usize
    }
    pub fn get_current_address(&self) -> usize {
        BigEndian::read_u24(&self.reg[0x20..0x23]) as usize
    }

    pub fn parse_cmd(
        &mut self,
        reader: &mut dyn BufRead,
    ) -> Result<DLDecoderResult, std::io::Error> {
        match reader.read_u8() {
            Ok(n) => {
                if n != 0xaf {
                    return Ok(DLDecoderResult::NOOP);
                }
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::UnexpectedEof => Err(e)?,
                _ => panic!("Cannot read: {}", e),
            },
        };
        match reader.read_u8().unwrap() {
            // set register
            0x20 => self.setreg(reader),

            // fill 8bit
            0x61 => self.fill8(reader),

            // memcpy 8bit
            0x62 => self.memcopy8(reader),

            // fill 16bit
            0x69 => self.fill16(reader),

            // memcpy 16bit
            0x6a => self.memcopy16(reader),

            // decompress 8bit
            0x70 => self.decomp8(reader),

            // decompress 16bit
            0x78 => self.decomp16(reader),

            // load decompression table
            0xe0 => self.load_decomp(reader),

            0xa0 => Ok(DLDecoderResult::NOOP),
            i => {
                panic!("Unexpected command: {:x}", i)
            }
        }
    }

    pub fn setreg(&mut self, reader: &mut dyn BufRead) -> Result<DLDecoderResult, std::io::Error> {
        let addr = reader.read_u8()?;
        let val = reader.read_u8()?;
        self.reg[addr as usize] = val;
        Ok(DLDecoderResult::SETREG(addr, val))
    }

    pub fn load_decomp(
        &mut self,
        reader: &mut dyn BufRead,
    ) -> Result<DLDecoderResult, std::io::Error> {
        reader.consume(4);
        let cnt = reader.read_u32::<BigEndian>()?;
        let mut nodebuf = [0u8; 9];
        for i in 0..cnt {
            reader.read_exact(&mut nodebuf)?;
            self.decomp_table[i as usize] = DecompNode::read_from(&nodebuf);
        }
        Ok(DLDecoderResult::NOOP)
    }

    pub fn memcopy8(
        &mut self,
        reader: &mut dyn BufRead,
    ) -> Result<DLDecoderResult, std::io::Error> {
        let dstaddr = reader.read_u24::<BigEndian>()? as usize;
        let cnt = wrap256(reader.read_u8()?);
        let srcaddr = reader.read_u24::<BigEndian>()? as usize;
        self.gfxram.copy_within(srcaddr..srcaddr + cnt, dstaddr);
        Ok(DLDecoderResult::MEMCPY(dstaddr, cnt))
    }

    pub fn memcopy16(
        &mut self,
        reader: &mut dyn BufRead,
    ) -> Result<DLDecoderResult, std::io::Error> {
        let dstaddr = reader.read_u24::<BigEndian>()? as usize;
        let cnt = wrap256(reader.read_u8()?) * 2;
        let srcaddr = reader.read_u24::<BigEndian>()? as usize;
        self.gfxram.copy_within(srcaddr..srcaddr + cnt, dstaddr);
        Ok(DLDecoderResult::MEMCPY(dstaddr, cnt))
    }

    pub fn fill8(&mut self, reader: &mut dyn BufRead) -> Result<DLDecoderResult, std::io::Error> {
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
        Ok(DLDecoderResult::FILL(addr, totalcnt))
    }

    pub fn fill16(&mut self, reader: &mut dyn BufRead) -> Result<DLDecoderResult, std::io::Error> {
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
        Ok(DLDecoderResult::FILL(addr, totalcnt))
    }

    pub fn decomp8(&mut self, reader: &mut dyn BufRead) -> Result<DLDecoderResult, std::io::Error> {
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
        Ok(DLDecoderResult::DECOMP(addr, cnt))
    }

    pub fn decomp16(
        &mut self,
        reader: &mut dyn BufRead,
    ) -> Result<DLDecoderResult, std::io::Error> {
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
        Ok(DLDecoderResult::DECOMP(addr, cnt))
    }
}
