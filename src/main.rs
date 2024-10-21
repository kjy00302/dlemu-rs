use sdl2::{
    event::Event, keyboard::Keycode, pixels::Color, pixels::PixelFormatEnum, render::Texture,
};
use std::env::args;
use std::fs::File;
use std::io::BufReader;
use std::sync::mpsc::{sync_channel, TryRecvError};
use std::thread;
use std::thread::sleep;
use std::time::Duration;

mod dldecoder;
use dldecoder::{DLDecoder, DLDecoderResult};

struct Frame {
    size: (u32, u32),
    data: Vec<u8>,
    addr: usize,
    dbg: Vec<DLDecoderResult>,
}

const FRAME_DURATION: Duration = Duration::new(0, 1_000_000_000u32 / 60);

fn main() {
    let (sender, receiver) = sync_channel::<Frame>(10);
    let bulkstream_f = File::open(args().nth(1).unwrap()).expect("Failed to open bulkstream");
    thread::spawn(move || {
        let mut bulkstream = BufReader::new(bulkstream_f);
        let mut decoder_ctx = DLDecoder::default();
        let mut dbg = vec![];

        'run: loop {
            if let Ok(result) = decoder_ctx.parse_cmd(&mut bulkstream) {
                match result {
                    DLDecoderResult::SETREG(addr, val) => {
                        match addr {
                            0xff => {
                                if val == 0xff && decoder_ctx.get_reg(0x1f) == 0 {
                                    // display new frame
                                    let addr = decoder_ctx.get_current_address();
                                    let w = decoder_ctx.get_width();
                                    let h = decoder_ctx.get_height();
                                    let len = w * h * 2;
                                    let mut data = vec![0u8; len];
                                    decoder_ctx.dumpbuffer(&mut data, addr, len);
                                    if sender
                                        .send(Frame {
                                            size: (w as u32, h as u32),
                                            data,
                                            addr,
                                            dbg,
                                        })
                                        .is_err()
                                    {
                                        break 'run;
                                    }
                                    dbg = vec![];
                                }
                            }
                            _ => {}
                        }
                    }
                    DLDecoderResult::FILL(_, _, true)
                    | DLDecoderResult::MEMCPY(_, _, true)
                    | DLDecoderResult::DECOMP(_, _, true) => {
                        dbg.push(result);
                    }
                    _ => {}
                }
            } else {
                break 'run;
            }
        }
        println!("decode thread finished");
    });

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
    let mut debugtex: Option<Texture> = None;
    let mut event_pump = sdl_context.event_pump().unwrap();
    let mut cur_size = (0, 0);
    let mut playing = true;
    let mut stepping = false;
    let mut draw_debug = false;
    'mainloop: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'mainloop,
                Event::KeyDown {
                    keycode,
                    repeat: false,
                    ..
                } => match keycode {
                    Some(Keycode::Space) => playing = !playing,
                    Some(Keycode::D) => draw_debug = !draw_debug,
                    Some(Keycode::Period) => {
                        playing = false;
                        stepping = true;
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        if playing | stepping {
            match receiver.try_recv() {
                Ok(frame) => {
                    if frame.size != cur_size {
                        let (w, h) = frame.size;
                        canvas.window_mut().set_size(w, h).unwrap();
                        rendertex = Some(
                            texture_creator
                                .create_texture_streaming(PixelFormatEnum::RGB565, w, h)
                                .unwrap(),
                        );
                        debugtex = Some({
                            let mut tex = texture_creator
                                .create_texture_target(PixelFormatEnum::RGBA8888, w, h)
                                .unwrap();
                            tex.set_blend_mode(sdl2::render::BlendMode::Add);
                            tex
                        });
                        println!("output resize: {}x{}", w, h);
                        cur_size = frame.size;
                    }
                    if let Some(tex) = &mut rendertex {
                        tex.with_lock(None, |buffer: &mut [u8], _pitch: usize| {
                            buffer.copy_from_slice(&frame.data);
                        })
                        .unwrap();
                    }
                    if let Some(tex) = &mut debugtex {
                        canvas
                            .with_texture_canvas(tex, |c| {
                                c.set_draw_color(Color::RGBA(0, 0, 0, 0));
                                c.clear();
                                for i in &frame.dbg {
                                    let color = match i {
                                        DLDecoderResult::FILL(_, _, true) => {
                                            Color::RGBA(255, 0, 0, 51)
                                        }
                                        DLDecoderResult::DECOMP(_, _, true) => {
                                            Color::RGBA(0, 255, 0, 51)
                                        }
                                        DLDecoderResult::MEMCPY(_, _, true) => {
                                            Color::RGBA(0, 0, 255, 51)
                                        }
                                        _ => Color::RGBA(0, 0, 0, 0),
                                    };
                                    c.set_draw_color(color);
                                    match i {
                                        DLDecoderResult::FILL(addr, len, _)
                                        | DLDecoderResult::DECOMP(addr, len, _)
                                        | DLDecoderResult::MEMCPY(addr, len, _) => {
                                            let width = frame.size.0 as i32;
                                            let start = ((addr - frame.addr) >> 1) as i32;
                                            let len = *len as i32;
                                            if width > 0 {
                                                let x = start % width;
                                                let y = start / width;
                                                let end = x + len - 1;
                                                c.draw_line((x, y), (end, y)).unwrap();
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            })
                            .unwrap();
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    break;
                }
            }
            stepping = false;
        }

        if let Some(tex) = &mut rendertex {
            canvas.copy(&tex, None, None).unwrap();
        }
        if draw_debug {
            if let Some(tex) = &mut debugtex {
                canvas.copy(&tex, None, None).unwrap();
            }
        }
        canvas.present();
        sleep(FRAME_DURATION);
    }
    println!("loop finished");
    // let mut ramdump = File::create_new("ramdump").unwrap();
    // ramdump.write(&gfxram).unwrap();
}
