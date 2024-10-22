use sdl2::{
    pixels::PixelFormatEnum,
    pixels::{Color, Palette},
    render::Texture,
    surface::Surface,
};

mod font8x8;
use font8x8::FONT8X8_BASIC;

pub fn generate_font_texture<T>(
    tc: &sdl2::render::TextureCreator<T>,
    fore: Color,
    back: Color,
) -> Texture<'_> {
    let mut a = FONT8X8_BASIC;
    let mut b = a
        .as_flattened_mut()
        .iter()
        .map(|&i| i.reverse_bits())
        .collect::<Vec<u8>>();
    let mut surf = Surface::from_data(&mut b, 8, 1024, 1, PixelFormatEnum::Index1MSB).unwrap();
    let pallete = Palette::with_colors(&[fore, back]).unwrap();
    surf.set_palette(&pallete).unwrap();
    surf.as_texture(tc).unwrap()
}

pub fn draw_text<T: sdl2::render::RenderTarget>(
    canvas: &mut sdl2::render::Canvas<T>,
    font: &sdl2::render::Texture,
    pos: sdl2::rect::Point,
    string: &str,
) {
    let mut x = pos.x;
    for c in string.chars() {
        let c: u32 = c.into();
        canvas
            .copy(
                font,
                Some((0, 8 * c as i32, 8, 8).into()),
                Some((x, pos.y, 8, 8).into()),
            )
            .unwrap();
        x += 8;
    }
}
