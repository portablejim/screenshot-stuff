extern crate image;

use std::env;
use std::path::Path;
use std::fs::File;
use image::GenericImage;
use image::Rgba;
use image::Pixel;

extern crate imagefmt;
use imagefmt::{ColFmt, ColType};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() == 4 {

        let imga = image::open(&Path::new(args.get(1).expect("No first image")));
        let imgb = image::open(&Path::new(args.get(2).expect("No second image")));

        match (imga, imgb) {
            (Err(_), Err(_)) => return,
            (Ok(_), Err(_)) => return,
            (Err(_), Ok(_)) => return,
            (Ok(imga), Ok(imgb)) => {
                let (w, h) = imga.dimensions();
                println!("{} {}", w, h);

                let mut imgc = image::DynamicImage::new_rgba8(w, h);

                for x in 0..w {
                    for y in 0..h {
                        let pixel_a = imga.get_pixel(x, y);
                        let pixel_b = imgb.get_pixel(x, y);

                        if pixel_a == pixel_b {
                            imgc.put_pixel(x, y, Rgba::from_channels(0, 0, 0, 0));
                        } else {
                            imgc.put_pixel(x, y, pixel_b)
                        }
                    }
                }

                let _ = imagefmt::write(
                    args.get(3).expect("No third image"),
                    w as usize,
                    h as usize,
                    ColFmt::RGBA,
                    &imgc.raw_pixels(),
                    ColType::ColorAlpha,
                ).unwrap();
            }

        };
    }
}
