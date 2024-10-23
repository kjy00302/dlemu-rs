use sdl2::{
    event::Event, keyboard::Keycode, pixels::Color, pixels::PixelFormatEnum, render::Texture,
};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, TryRecvError};
use std::thread;
use std::thread::sleep;
use std::time::Duration;

use clap::Parser;

mod dldecoder;
use dldecoder::{DLDecoder, DLDecoderResult};

mod drawtext;
use drawtext::{draw_text, generate_font_texture};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    debugdraw: bool,

    #[arg(short, long)]
    info: bool,

    #[arg(short, long)]
    pause: bool,

    #[arg(short, long, default_value_t = 60)]
    fps: u32,

    #[arg(long, default_value_t = 10)]
    buffersize: usize,

    #[arg(value_name = "FILE")]
    path: PathBuf,
}

struct Frame {
    size: (u32, u32),
    data16: Vec<u8>,
    data8: Vec<u8>,
    addr16: usize,
    addr8: usize,
    dbg: Vec<DLDecoderResult>,
    reg: [u8; 256],
}

fn main() {
    let args = Args::parse();
    let frame_duration = Duration::new(0, 1_000_000_000u32 / args.fps);
    let (sender, receiver) = sync_channel::<Frame>(args.buffersize);
    let bulkstream_f = File::open(args.path).expect("Failed to open bulkstream");

    thread::spawn(move || {
        let mut bulkstream = BufReader::new(bulkstream_f);
        let mut decoder_ctx = DLDecoder::default();
        let mut dbg = vec![];

        while let Ok(result) = decoder_ctx.parse_cmd(&mut bulkstream) {
            match result {
                DLDecoderResult::Setreg(addr, val) => {
                    if addr == 0xff && val == 0xff && decoder_ctx.get_reg(0x1f) == 0 {
                        // display new frame
                        let addr16 = decoder_ctx.get_current_address_16();
                        let addr8 = decoder_ctx.get_current_address_8();
                        let w = decoder_ctx.get_width();
                        let h = decoder_ctx.get_height();
                        let len = w * h;
                        let mut data16 = vec![0u8; len * 2];
                        let mut data8 = vec![0u8; len];
                        let mut reg = [0u8; 256];
                        decoder_ctx.dumpbuffer(&mut data16, addr16, len * 2);
                        decoder_ctx.dumpbuffer(&mut data8, addr8, len);
                        decoder_ctx.dumpreg(&mut reg);
                        if sender
                            .send(Frame {
                                size: (w as u32, h as u32),
                                data16,
                                data8,
                                addr16,
                                addr8,
                                dbg,
                                reg,
                            })
                            .is_err()
                        {
                            break;
                        }
                        dbg = vec![];
                    }
                }
                DLDecoderResult::Fill(_, _, true)
                | DLDecoderResult::Memcpy(_, _, true)
                | DLDecoderResult::Decomp(_, _, true) => {
                    dbg.push(result);
                }
                _ => {}
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
    let font = {
        let mut t = generate_font_texture(&texture_creator, Color::BLACK, Color::WHITE);
        t.set_blend_mode(sdl2::render::BlendMode::Blend);
        t.set_alpha_mod(180);
        t
    };
    let mut event_pump = sdl_context.event_pump().unwrap();
    let mut cur_size = (0, 0);
    let mut framecnt = 0;
    let mut reg_localcopy = [0u8; 256];
    let mut addr = (0, 0);
    let mut playing = !args.pause;
    let mut stepping = false;
    let mut draw_debug = args.debugdraw;
    let mut show_info = args.info;
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
                    Some(Keycode::I) => show_info = !show_info,
                    Some(Keycode::Q) => break 'mainloop,
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
                                .create_texture_streaming(PixelFormatEnum::RGB888, w, h)
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
                            for i in 0..frame.data8.len() {
                                let b = frame.data8[i];
                                let h = (frame.data16[i * 2 + 1] as u16) << 8
                                    | frame.data16[i * 2] as u16;
                                buffer[i * 4 + 2] = ((h & 0xf800) >> 8) as u8 | b >> 5;
                                buffer[i * 4 + 1] = ((h & 0x7e0) >> 3) as u8 | ((b >> 3) & 3);
                                buffer[i * 4] = ((h & 0x1f) << 3) as u8 | b & 7;
                            }
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
                                        DLDecoderResult::Fill(_, _, true) => {
                                            Color::RGBA(255, 0, 0, 51)
                                        }
                                        DLDecoderResult::Decomp(_, _, true) => {
                                            Color::RGBA(0, 255, 0, 51)
                                        }
                                        DLDecoderResult::Memcpy(_, _, true) => {
                                            Color::RGBA(0, 0, 255, 51)
                                        }
                                        _ => Color::RGBA(0, 0, 0, 0),
                                    };
                                    c.set_draw_color(color);
                                    match i {
                                        DLDecoderResult::Fill(addr, len, _)
                                        | DLDecoderResult::Decomp(addr, len, _)
                                        | DLDecoderResult::Memcpy(addr, len, _) => {
                                            let width = frame.size.0 as i32;
                                            let start = ((addr - frame.addr16) >> 1) as i32;
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
                    reg_localcopy = frame.reg;
                    addr = (frame.addr16, frame.addr8);
                    framecnt += 1;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    break;
                }
            }
            stepping = false;
        }

        if let Some(tex) = &mut rendertex {
            canvas.copy(tex, None, None).unwrap();
        }
        if draw_debug {
            if let Some(tex) = &mut debugtex {
                canvas.copy(tex, None, None).unwrap();
            }
        }
        if show_info {
            draw_text(
                &mut canvas,
                &font,
                (0, 0).into(),
                &format!("frame: {}", framecnt),
            );
            draw_text(
                &mut canvas,
                &font,
                (0, 8).into(),
                &format!("addr16: {:06X}, addr8: {:06X}", addr.0, addr.1),
            );
            for i in 0..256 {
                let x = i % 16;
                let y = i / 16;
                draw_text(
                    &mut canvas,
                    &font,
                    (x * 16, y * 8 + 16).into(),
                    &format!("{:02X}", reg_localcopy[i as usize]),
                );
            }
        }

        canvas.present();
        sleep(frame_duration);
    }
    println!("loop finished");
    // let mut ramdump = File::create_new("ramdump").unwrap();
    // ramdump.write(&gfxram).unwrap();
}
