#[path = "../src/noise/opensimplex2s.rs"]
mod opensimplex2s;

use image::{GrayImage, Luma};

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 1024;

fn main() {
    let mut image = GrayImage::new(WIDTH, HEIGHT);

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let nx = x as f32 / 100.0;
            let ny = y as f32 / 100.0;

            let value = opensimplex2s::noise2(0, nx, ny);

            // Map [-1, 1] -> [0, 255]
            let value = ((value + 1.0) * 0.5 * 255.0) as u8;
            image.put_pixel(x, y, Luma([value]));
        }
    }

    image.save("noise.png").unwrap();
}
