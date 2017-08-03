extern crate image;
extern crate serde_json;
extern crate oxipng;
//extern crate imagequant;
extern crate png;

use std::env;
use std::path::Path;
use std::fs::File;
use std::fs;
use std::char;
use std::io::Read;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::ascii::AsciiExt;
use image::GenericImage;
use image::Rgba;
use image::Pixel;
use image::DynamicImage;
use std::io::Write;

extern crate imagefmt;
use imagefmt::{ColFmt, ColType};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() == 2 {
        let image_hashes: HashMap<usize, String> = HashMap::new();

        let mut timings_file: String = String::new();
        File::open(args.get(1).expect("Error getting path")).expect("No such file").read_to_string(&mut timings_file);
        let timings_dir = Path::new(args.get(1).expect("Error getting timings arg")).parent().unwrap_or(Path::new("."));
        println!("{:?}", timings_dir);
        let timings: Vec<Vec<String>> = serde_json::from_slice(timings_file.as_ref()).unwrap_or(vec![]);
        let timings_new: Vec<Vec<String>> = vec![];

        let images_path = timings_dir.join("images");
        if !images_path.is_dir() && fs::create_dir(&images_path).is_err() {
            println!("Error: Can't use 'images' directory");
            return;
        }

        let mut previous: Option<&DynamicImage> = None;
        let mut entry_num = 0;
        for entry_num in 2..timings.len() {
            let entry = timings.get(entry_num).expect("Weird length for timings");
            if entry.len() >= 2 {
                let entry_image = timings_dir.join(entry.get(1).expect("No path supplied"));
                match previous {
                    None => {
                        // First entry
                        match image::open(timings_dir.join(entry_image)) {
                            Ok(image) => {
                                let optimised_image = optimise(image, 75);
                                let px =  (optimised_image.to_rgb());//.into_raw();

                                let mut oxioptions = oxipng::Options::from_preset(4);
                                oxioptions.interlace = Some(1);
                                oxioptions.verbosity = Some(1);
                                let out_name = format!("slide{:03}.png", entry_num);
                                let out_relpath = match images_path.file_name().and_then(|n|n.to_str()) {
                                    Some(images_path_name) => format!("{}/{}", images_path_name, out_name),
                                    None => String::new()
                                };


                                let mut image_vec: Vec<u8> = Vec::new();
                                let image_writer = png::Writer::new(image_hashes);

                                oxioptions.out_file = images_path.join(&out_name);
                                println!("out: {}", oxioptions.out_file.to_str().unwrap_or("(none)"));
                                let oxi_output = oxipng::optimize_from_memory(&px, &oxioptions).expect("Error creating compressed image");
                                File::create(images_path.join(&out_name)).expect("Error writing").write(&oxi_output);
                                //}
                            }
                            Err(e) => {
                                eprintln!("Error when reading {:?}", e);
                            }
                        }
                    }
                    Some(previousEntry) => {

                    }
                }
            }
        }
        /*
        let dira = fs::read_dir(Path::new(args.get(1).expect("Error getting directory"))).expect("Error getting directory contents");
        let png_list: Vec<_> = dira.filter_map(|d|d.ok().and_then(|e| Some(e.file_name()))).filter(|entry| {
            entry.to_str().unwrap_or("").to_ascii_lowercase().ends_with(".png")
        }).collect();
        println!("{:?}", png_list);*/
    }
    return;
}

fn optimise(image: DynamicImage, min_quality: u8) -> DynamicImage {
    /*let mut liq = imagequant::new();
    liq.set_quality(min_quality, 100);
    liq.set_speed(1);*/

    //let ref mut liq_image = liq.new_image(image, image.width(), image.height())
    return image;
}


fn diff2(patha: String, pathb: String, pathc: String) {

        let imga = image::open(&Path::new(&patha));
        let imgb = image::open(&Path::new(&pathb));

        match (imga, imgb) {
            (Err(_), Err(_)) => return,
            (Ok(_), Err(_)) => return,
            (Err(_), Ok(_)) => return,
            (Ok(imga), Ok(imgb)) => {
                let (w, h) = imga.dimensions();

                let mut imgc = image::DynamicImage::new_rgba8(w, h);

                let mut pixels_same: u64 = 0;
                let mut pixels_notsame: u64 = 0;
                let mut colours = BTreeSet::new();
                for x in 0..w {
                    for y in 0..h {
                        let pixel_a = imga.get_pixel(x, y);
                        let pixel_b = imgb.get_pixel(x, y);

                        if pixel_a == pixel_b {
                            imgc.put_pixel(x, y, Rgba::from_channels(0, 0, 0, 0));
                            pixels_same += 1;
                        } else {
                            imgc.put_pixel(x, y, pixel_b);
                            let colour_value = ((pixel_b.data[0] as u32) << 16) + ((pixel_b.data[1] as u32) << 8) + pixel_b[2] as u32;
                            pixels_notsame += 1;
                            if !colours.contains(&colour_value) && colours.len() < 300 {
                                colours.insert(colour_value);
                            }
                        }
                    }
                }
                let pixels_same_percent = (pixels_same * 100) / (pixels_notsame + pixels_same);
                let num_colours = match colours.len() {
                    300 => format!("300+"),
                    other => format!("{}", other)
                };
                println!("Transparent: {}, Colours: {}", pixels_same_percent, num_colours);

                if pixels_notsame == 0 {
                    // No transparency no alpha channel is just fluff.
                    //imgc = imgc.to_rgb()
                    let _ = imagefmt::write(
                        pathc,
                        w as usize,
                        h as usize,
                        ColFmt::RGB,
                        &imgc.raw_pixels(),
                        ColType::Color,
                    ).unwrap();
                }
                else {
                    let _ = imagefmt::write(
                        pathc,
                        w as usize,
                        h as usize,
                        ColFmt::RGBA,
                        &imgc.raw_pixels(),
                        ColType::ColorAlpha,
                    ).unwrap();
                }

            }

        };
}
