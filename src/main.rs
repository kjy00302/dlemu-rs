use byteorder::{BigEndian, ByteOrder, ReadBytesExt};
use sdl2::{event::Event, pixels::PixelFormatEnum, render::Texture};
use std::env::args;
use std::fs::File;
use std::io::{prelude::*, BufReader, ErrorKind};
use std::thread::sleep;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
struct DecompEntry {
    color: u16,
    nextjump: usize,
}

impl DecompEntry {
    fn read_from<R: Read>(r: &mut R) -> [DecompEntry; 2] {
        let color_a = r.read_u16::<BigEndian>().unwrap();
        let _a = r.read_u8().unwrap();
        let a = r.read_u8().unwrap();
        let ab = r.read_u8().unwrap();
        let color_b = r.read_u16::<BigEndian>().unwrap();
        let _b = r.read_u8().unwrap();
        let b = r.read_u8().unwrap();

        [
            DecompEntry {
                color: color_a,
                nextjump: ((a & 0x1f) as usize) << 4 | (ab >> 4) as usize,
            },
            DecompEntry {
                color: color_b,
                nextjump: ((b & 0x1f) as usize) << 4 | (ab & 0xf) as usize,
            },
        ]
    }
}

fn wrap256(n: u8) -> usize {
    if n == 0 {
        256
    } else {
        n as usize
    }
}

const FRAME_DURATION: Duration = Duration::new(0, 1_000_000_000u32 / 30);

fn main() {
    let mut decomp_table = [[
        DecompEntry {
            color: 0,
            nextjump: 0,
        },
        DecompEntry {
            color: 0,
            nextjump: 0,
        },
    ]; 512];

    let mut reg = [0u8; 0x100];
    let mut gfxram = vec![0u8; 0x100_0000];

    let bulkstream_f = File::open(args().nth(1).unwrap()).expect("Failed to open bulkstream");
    let mut bulkstream = BufReader::new(bulkstream_f);

    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();

    let window = video_subsystem
        .window("dlemu-rs", 1280, 1024)
        .position_centered()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas().build().unwrap();
    let texture_creator = canvas.texture_creator();
    let mut rendertex: Option<Texture> = None;
    let mut event_pump = sdl_context.event_pump().unwrap();
    let mut last_time = Instant::now();
    'mainloop: loop {
        for event in event_pump.poll_iter() {
            if let Event::Quit { .. } = event {
                break 'mainloop;
            }
        }
        match bulkstream.read_u8() {
            Ok(n) => {
                if n != 0xaf {
                    continue;
                }
            }
            Err(e) => match e.kind() {
                ErrorKind::UnexpectedEof => break,
                _ => panic!("Cannot read: {}", e),
            },
        };
        match bulkstream.read_u8().unwrap() {
            0x20 => {
                // set register
                let addr = bulkstream.read_u8().unwrap();
                let val = bulkstream.read_u8().unwrap();
                reg[addr as usize] = val;
                match addr {
                    0x18 => {
                        // resize surface
                        let w = BigEndian::read_u16(&reg[0x0f..0x11]) as u32;
                        let h = BigEndian::read_u16(&reg[0x17..0x19]) as u32;
                        canvas.window_mut().set_size(w, h).unwrap();
                        rendertex = Some(
                            texture_creator
                                .create_texture_streaming(PixelFormatEnum::RGB565, w, h)
                                .unwrap(),
                        );
                        println!("output resize: {}x{}", w, h);
                    }
                    0xff => {
                        if val == 0xff && reg[0x1f] == 0 {
                            // display new frame
                            let addr = BigEndian::read_u24(&reg[0x20..0x23]) as usize;
                            if let Some(tex) = &mut rendertex {
                                tex.with_lock(None, |buffer: &mut [u8], _pitch: usize| {
                                    buffer.copy_from_slice(&gfxram[addr..addr + buffer.len()]);
                                })
                                .unwrap();
                                canvas.copy(tex, None, None).unwrap();
                            }
                            canvas.present();
                            let now = Instant::now();
                            let delta = now - last_time;
                            last_time = now;
                            if delta > FRAME_DURATION {
                                if delta.subsec_millis() > FRAME_DURATION.subsec_millis() + 5 {
                                    println!(
                                        "framerate drop: {}ms > {}ms",
                                        delta.subsec_millis(),
                                        FRAME_DURATION.subsec_millis()
                                    );
                                }
                            } else {
                                sleep(FRAME_DURATION - delta);
                            }
                        }
                    }
                    _ => {}
                }
            }
            0x61 => {
                // fill 8bit
                let mut addr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                let mut totalcnt = wrap256(bulkstream.read_u8().unwrap());
                while totalcnt > 0 {
                    let cnt = wrap256(bulkstream.read_u8().unwrap());
                    let value = bulkstream.read_u8().unwrap();
                    for i in 0..cnt {
                        gfxram[addr + i] = value;
                    }
                    totalcnt -= cnt;
                    addr += cnt;
                }
            }
            0x62 => {
                // memcpy 8bit
                let dstaddr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                let cnt = wrap256(bulkstream.read_u8().unwrap());
                let srcaddr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                gfxram.copy_within(srcaddr..srcaddr + cnt, dstaddr);
            }
            0x69 => {
                // fill 16bit
                let mut addr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                let mut totalcnt = wrap256(bulkstream.read_u8().unwrap());
                while totalcnt > 0 {
                    let cnt = wrap256(bulkstream.read_u8().unwrap());
                    let mut value = [0u8; 2];
                    bulkstream.read_exact(&mut value).unwrap();
                    for i in 0..cnt {
                        gfxram[addr + i * 2] = value[1];
                        gfxram[addr + i * 2 + 1] = value[0];
                    }
                    totalcnt -= cnt;
                    addr += cnt * 2;
                }
            }
            0x6a => {
                // memcpy 16bit
                let dstaddr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                let cnt = wrap256(bulkstream.read_u8().unwrap());
                let srcaddr = bulkstream.read_u24::<BigEndian>().unwrap() as usize;
                gfxram.copy_within(srcaddr..srcaddr + cnt * 2, dstaddr);
            }
            0x70 => {
                // decompress 8bit
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
                        let decomp_entry = &decomp_table[tableidx][(bytebuf & 1) as usize];
                        bytebuf >>= 1;
                        bitcnt += 1;
                        loop_acc = loop_acc.wrapping_add((decomp_entry.color & 0xff) as u8);
                        tableidx = decomp_entry.nextjump;
                        if tableidx == 0 {
                            break;
                        }
                    }
                    color_acc = color_acc.wrapping_add(loop_acc);
                    gfxram[addr] = color_acc;
                    addr += 1;
                    tableidx = 0;
                }
            }
            0x78 => {
                // decompress 16bit
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
                        let decomp_entry = &decomp_table[tableidx][(bytebuf & 1) as usize];
                        bytebuf >>= 1;
                        bitcnt += 1;
                        loop_acc = loop_acc.wrapping_add(decomp_entry.color);
                        tableidx = decomp_entry.nextjump;
                        if tableidx == 0 {
                            break;
                        }
                    }
                    color_acc = color_acc.wrapping_add(loop_acc);
                    gfxram[addr + 1] = (color_acc >> 8) as u8;
                    gfxram[addr] = (color_acc & 0xff) as u8;
                    addr += 2;
                    tableidx = 8;
                }
            }
            0xe0 => {
                // load decompression table
                bulkstream.consume(4);
                let cnt = bulkstream.read_u32::<BigEndian>().unwrap();
                for i in 0..cnt {
                    decomp_table[i as usize] = DecompEntry::read_from(&mut bulkstream);
                }
            }
            0xa0 => {}
            i => {
                panic!("Unexpected command: {:x}", i)
            }
        }
    }
    println!("loop finished");
    // let mut ramdump = File::create_new("ramdump").unwrap();
    // ramdump.write(&gfxram).unwrap();
}
