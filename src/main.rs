use byteorder::{BigEndian, ByteOrder, ReadBytesExt};
use sdl2::{event::Event, pixels::PixelFormatEnum, render::Texture};
use std::env::args;
use std::fs::File;
use std::io::{BufReader, ErrorKind};
use std::thread::sleep;
use std::time::{Duration, Instant};

mod dldecoder;
use dldecoder::DLDecoder;

const FRAME_DURATION: Duration = Duration::new(0, 1_000_000_000u32 / 30);

fn main() {
    let mut decoder_ctx = DLDecoder::default();

    let mut reg = [0u8; 0x100];

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
                                    decoder_ctx.dumpbuffer(buffer, addr, buffer.len());
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
                decoder_ctx.fill8(&mut bulkstream);
            }
            0x62 => {
                // memcpy 8bit
                decoder_ctx.memcopy8(&mut bulkstream);
            }
            0x69 => {
                // fill 16bit
                decoder_ctx.fill16(&mut bulkstream);
            }
            0x6a => {
                // memcpy 16bit
                decoder_ctx.memcopy16(&mut bulkstream);
            }
            0x70 => {
                // decompress 8bit
                decoder_ctx.decomp8(&mut bulkstream);
            }
            0x78 => {
                // decompress 16bit
                decoder_ctx.decomp16(&mut bulkstream);
            }
            0xe0 => {
                // load decompression table
                decoder_ctx.load_decomp(&mut bulkstream);
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
