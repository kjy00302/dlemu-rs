use std::env::args;
use std::fs::File;
use std::io::{prelude::*, BufReader, ErrorKind};
use byteorder::{ReadBytesExt, BigEndian};

#[derive(Clone, Copy, Debug)]
struct DecompEntry {
    color: u16,
    nextjump: usize
}

impl DecompEntry {
    fn read_from<R: Read>(r: &mut R) -> (DecompEntry, DecompEntry) {
        let color_a = r.read_u16::<BigEndian>().unwrap();
        let _a = r.read_u8().unwrap();
        let a = r.read_u8().unwrap();
        let ab = r.read_u8().unwrap();
        let color_b = r.read_u16::<BigEndian>().unwrap();
        let _b = r.read_u8().unwrap();
        let b = r.read_u8().unwrap();

        return (DecompEntry {
            color: color_a,
            nextjump: ((a & 0x1f) as usize) << 4 | (ab >> 4) as usize
        }, DecompEntry {
            color: color_b,
            nextjump: ((b & 0x1f) as usize) << 4 | (ab & 0xf) as usize
        })
    }
}

#[inline]
fn wrap256(n:u8) -> usize {
    if n == 0 {256}
    else {n as usize}
}

fn main() {
    
    let mut decomp_table = [(
        DecompEntry{color:0, nextjump:0},
        DecompEntry{color:0, nextjump:0}
    ); 512];

    let mut reg = [0u8; 0x100];
    let mut gfxram = vec![0u8; 0x100_0000];

    let bulkstream_f = File::open(args().nth(1).unwrap())
        .expect("Failed to open bulkstream");
    let mut bulkstream = BufReader::new(bulkstream_f);
    loop {
        match bulkstream.read_u8() {
            Ok(n) => if n != 0xaf {continue;},
            Err(e) => match e.kind() {
                ErrorKind::UnexpectedEof => break,
                _ => panic!("Cannot read: {}", e)
            }
        };
        match bulkstream.read_u8().unwrap() {
            0x20 => {
                let addr = bulkstream.read_u8().unwrap();
                let val = bulkstream.read_u8().unwrap();
                reg[addr as usize] = val;
                if addr == 0xff && val == 0xff && reg[0x1f] == 0 {
                    println!("frame");
                }
            }
            0x61 => {
                let mut addr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                let mut totalcnt = wrap256(bulkstream.read_u8().unwrap());
                while totalcnt > 0 {
                    let cnt = wrap256(bulkstream.read_u8().unwrap());
                    let value = bulkstream.read_u8().unwrap();
                    for i in 0..cnt {
                        gfxram[addr+i] = value;
                    }
                    totalcnt -= cnt;
                    addr += cnt;
                }
            }
            0x62 => {
                let dstaddr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                let cnt = wrap256(bulkstream.read_u8().unwrap());
                let srcaddr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                gfxram.copy_within(srcaddr..srcaddr+cnt, dstaddr);
            }
            0x69 => {
                let mut addr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                let mut totalcnt = wrap256(bulkstream.read_u8().unwrap());
                while totalcnt > 0 {
                    let cnt = wrap256(bulkstream.read_u8().unwrap());
                    let mut value = [0u8;2];
                    bulkstream.read_exact(&mut value).unwrap();
                    for i in 0..cnt {
                        gfxram[addr+i*2] = value[0];
                        gfxram[addr+i*2+1] = value[1];
                    }
                    totalcnt -= cnt;
                    addr += cnt*2;
                }
            }
            0x6a => {
                let dstaddr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                let cnt = wrap256(bulkstream.read_u8().unwrap());
                let srcaddr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                gfxram.copy_within(srcaddr..srcaddr+cnt*2, dstaddr);
            }
            0x70 => {
                let mut addr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                let cnt = wrap256(bulkstream.read_u8().unwrap());

                let mut tableidx = 0;
                let mut color_acc = 0u8;
                let mut bitcnt = 8;
                let mut bytebuf = 0;
                for _ in 0..cnt {
                    let mut loop_acc = 0u8;
                    loop {
                        if bitcnt == 8 {
                            bytebuf = bulkstream.read_u8().unwrap();
                            bitcnt = 0;
                        }
                        let decomp_entry = if bytebuf & 1 == 1 {decomp_table[tableidx].1} else {decomp_table[tableidx].0};
                        bytebuf >>= 1;
                        bitcnt += 1;
                        loop_acc = loop_acc.wrapping_add((decomp_entry.color & 0xff) as u8);
                        if decomp_entry.nextjump == 0 {
                            color_acc = color_acc.wrapping_add(loop_acc);
                            gfxram[addr] = color_acc;
                            addr += 1;
                            tableidx = 0;
                            break;
                        }
                        tableidx = decomp_entry.nextjump;
                    }
                }
            }
            0x78 => {
                let mut addr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                let cnt = wrap256(bulkstream.read_u8().unwrap());

                let mut tableidx = 8;
                let mut color_acc = 0u16;
                let mut bitcnt = 8;
                let mut bytebuf = 0;
                for _ in 0..cnt {
                    let mut loop_acc = 0u16;
                    loop {
                        if bitcnt == 8 {
                            bytebuf = bulkstream.read_u8().unwrap();
                            bitcnt = 0;
                        }
                        let decomp_entry = if bytebuf & 1 == 1 {decomp_table[tableidx].1} else {decomp_table[tableidx].0};
                        bytebuf >>= 1;
                        bitcnt += 1;
                        loop_acc = loop_acc.wrapping_add(decomp_entry.color);
                        if decomp_entry.nextjump == 0 {
                            color_acc = color_acc.wrapping_add(loop_acc);
                            gfxram[addr] = (color_acc >> 8) as u8;
                            gfxram[addr+1] = (color_acc & 0xff) as u8;
                            addr += 2;
                            tableidx = 8;
                            break;
                        }
                        tableidx = decomp_entry.nextjump;
                    }
                }
            }
            0xe0 => {
                bulkstream.consume(4);
                let cnt = bulkstream.read_u32::<BigEndian>().unwrap();
                for i in 0..cnt {
                    decomp_table[i as usize] = DecompEntry::read_from(&mut bulkstream);
                }
            }
            0xa0 => {},
            i => {panic!("Unexpected command: {:x}", i)}
        }
    }
    println!("loop finished");
}
