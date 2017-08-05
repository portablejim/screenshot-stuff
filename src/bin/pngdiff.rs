extern crate image;
extern crate serde_json;
extern crate oxipng;
//extern crate imagequant;
extern crate png;
extern crate twox_hash;

use std::env;
use std::path::Path;
use std::fs::File;
use std::fs;
use std::io::Read;
use std::fs::OpenOptions;
use image::{GenericImage, Rgba, Pixel, DynamicImage};
use image::DynamicImage::ImageRgb8;
use std::io::Write;
use std::collections::HashMap;
use twox_hash::XxHash;
use std::hash::Hasher;
use png::HasParameters;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() == 2 {
        let mut image_hashes: HashMap<u64, String> = HashMap::new();

        let mut timings_file: String = String::new();
        let timings_file_arg = args.get(1).expect("Error getting timings file argument");
        File::open(timings_file_arg)
            .expect("No such file")
            .read_to_string(&mut timings_file)
            .expect("Error reading timings file");
        let timings_dir = Path::new(args.get(1).expect("Error getting timings arg"))
            .parent()
            .unwrap_or(Path::new("."));
        println!("{:?}", timings_dir);
        let timings: Vec<Vec<String>> =
            serde_json::from_slice(timings_file.as_ref()).unwrap_or(vec![]);
        let mut timings_new: Vec<Vec<String>> = vec![];

        let images_path = timings_dir.join("images");
        if !images_path.is_dir() && fs::create_dir(&images_path).is_err() {
            println!("Error: Can't use 'images' directory");
            return;
        }

        let mut previous: Option<DynamicImage> = None;
        println!("{}", timings.len());
        for entry_num in 0..timings.len() {
            let entry = timings.get(entry_num).expect("Weird length for timings");
            if entry.len() >= 2 {
                let entry_image = timings_dir.join(entry.get(1).expect("No path supplied"));
                previous = match previous {
                    None => {
                        // First entry
                        match image::open(timings_dir.join(entry_image)) {
                            Ok(rgba_img) => {
                                let mut image_data_pre: DynamicImage = ImageRgb8(rgba_img.to_rgb());
                                // Generate hash
                                let mut hasher = XxHash::default();
                                for pixel in rgba_img.raw_pixels() {
                                    hasher.write_u8(pixel);
                                }
                                let hash_value = hasher.finish();

                                let (w, h) = image_data_pre.dimensions();
                                for y in 0..h {
                                    for x in 0..w {
                                        let pixel = image_data_pre.get_pixel(x, y);
                                        match (pixel[0], pixel[1], pixel[2]) {
                                            (0, 0, 0) => {
                                                image_data_pre.put_pixel(
                                                    x,
                                                    y,
                                                    Rgba::from_channels(1, 1, 1, 255),
                                                )
                                            }
                                            _ => (),
                                        }
                                    }
                                }
                                let image_data = image_data_pre;
                                let out_name = format!("slide{:03}.png", entry_num + 1);
                                let out_relpath =
                                    match images_path.file_name().and_then(|n| n.to_str()) {
                                        Some(images_path_name) => {
                                            format!("{}/{}", images_path_name, out_name)
                                        }
                                        None => String::new(),
                                    };
                                image_hashes.insert(hash_value, out_relpath.clone());
                                save_image(&images_path.join(out_name), &image_data, 0);

                                let mut entry_new: Vec<String> = entry.clone();
                                entry_new[1] = out_relpath;
                                timings_new.push(entry_new);

                                Some(image_data)
                            }
                            Err(e) => {
                                eprintln!("Error when reading {:?}", e);
                                None
                            }
                        }
                    }
                    Some(ref previous_entry) => {
                        match image::open(timings_dir.join(entry_image)) {
                            Ok(image_data) => {
                                // Generate hash
                                let mut hasher = XxHash::default();
                                for pixel in image_data.raw_pixels() {
                                    hasher.write_u8(pixel);
                                }
                                let hash_value = hasher.finish();
                                println!(
                                    "Hash: {}, seen: {}",
                                    hash_value,
                                    image_hashes.contains_key(&hash_value)
                                );

                                let (image_diff, diff_percent) = diff2(previous_entry, &image_data);

                                if image_hashes.contains_key(&hash_value) {
                                    let other_image_path = &image_hashes[&hash_value];
                                    match image::open(timings_dir.join(other_image_path)) {
                                        Ok(other_image_data_pre) => {
                                            let other_image_data =
                                                ImageRgb8(other_image_data_pre.to_rgb());
                                            let (image_addition, add_percent) =
                                                add2(other_image_data, &image_diff);

                                            let out_relpath = other_image_path;
                                            save_image(
                                                &timings_dir.join(other_image_path),
                                                &image_addition,
                                                add_percent,
                                            );

                                            let mut entry_new: Vec<String> = entry.clone();
                                            entry_new[1] = out_relpath.clone();
                                            timings_new.push(entry_new);

                                            Some(image_data)
                                        }
                                        Err(e) => {
                                            eprintln!("Error when reading old image: {:?}", e);
                                            None
                                        }
                                    }
                                } else {
                                    // Setup oxipng
                                    let out_name = format!("slide{:03}.png", entry_num + 1);
                                    let out_relpath =
                                        match images_path.file_name().and_then(|n| n.to_str()) {
                                            Some(images_path_name) => {
                                                format!("{}/{}", images_path_name, out_name)
                                            }
                                            None => String::new(),
                                        };
                                    image_hashes.insert(hash_value, out_relpath.clone());
                                    save_image(
                                        &images_path.join(out_name),
                                        &image_diff,
                                        diff_percent,
                                    );

                                    let mut entry_new: Vec<String> = entry.clone();
                                    entry_new[1] = out_relpath;
                                    timings_new.push(entry_new);

                                    Some(image_data)
                                }
                            }
                            Err(e) => {
                                eprintln!("Error when reading {:?}", e);
                                None
                            }
                        }
                    }
                }
            }
        }

        println!("New json: {:?}", timings_new);
        let timings_new_string =
            serde_json::to_string(&timings_new).expect("Error serialising new timings");
        let mut rewrite_options = OpenOptions::new();
        rewrite_options.write(true);
        rewrite_options.truncate(true);
        match rewrite_options.open(timings_file_arg).and_then(|mut f| {
            f.write_all(&timings_new_string.as_bytes())
        }) {
            Ok(_) => (),
            Err(e) => eprintln!("Error writing new json file: {:?}", e),
        }
    }
    return;
}

fn calc_percent_transparent(transparent: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }

    let raw_percent = (transparent * 100) / total;

    match (transparent, raw_percent) {
        (0, 0) => 0,
        (_, 0) => 1,
        (_, n) => n,
    }
}

fn save_image(out_path: &Path, input_image: &DynamicImage, percent_transparent: u64) {
    let trns_black_transparent: [u8; 6] = [0, 0, 0, 0, 0, 0];

    let mut oxioptions = oxipng::Options::from_preset(4);
    oxioptions.verbosity = Some(0);
    if percent_transparent < 30 {
        oxioptions.interlace = Some(1);
    }
    oxioptions.out_file = out_path.to_path_buf();
    oxioptions.bit_depth_reduction = false;
    oxioptions.color_type_reduction = false;

    // Save png with oxipng
    let mut image_vec: Vec<u8> = Vec::new();
    let (img_width, img_height) = input_image.dimensions();
    {
        let mut img_encoder = png::Encoder::new(&mut image_vec, img_width, img_height);
        img_encoder.set(png::ColorType::RGB).set(
            png::BitDepth::Eight,
        );
        let mut img_writer = img_encoder.write_header().expect("Problem writing headers");
        if percent_transparent == 0 {
            match img_writer.write_chunk(png::chunk::tRNS, &trns_black_transparent) {
                Ok(_) => (),
                Err(e) => eprintln!("Error writing tRNS header to temporary PNG: {:?}", e),
            }
        }
        match img_writer.write_image_data(&input_image.raw_pixels()) {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Error writing image data for temporary PNG: {:?}", e);
                return;
            }
        }
    }

    let oxi_output = oxipng::optimize_from_memory(&image_vec, &oxioptions)
        .expect("Error creating compressed image_data");
    match File::create(out_path).expect("Error writing").write(
        &oxi_output,
    ) {
        Ok(_) => (),
        Err(e) => eprintln!("Error writing optimised PNG: {:?}", e),
    }

}


fn diff2(imga: &DynamicImage, imgb: &DynamicImage) -> (DynamicImage, u64) {

    let (w, h) = imga.dimensions();

    let mut imgc = image::DynamicImage::new_rgb8(w, h);

    let mut pixels_same: u64 = 0;
    let mut pixels_notsame: u64 = 0;
    for y in 0..h {
        for x in 0..w {
            let pixel_a = imga.get_pixel(x, y);
            let pixel_b = imgb.get_pixel(x, y);

            if pixel_a == pixel_b {
                imgc.put_pixel(x, y, Rgba::from_channels(0, 0, 0, 0));
                pixels_same += 1;
            } else {
                let source_pixel = imgb.get_pixel(x, y);
                match (source_pixel[0], source_pixel[1], source_pixel[2]) {
                    (0, 0, 0) => imgc.put_pixel(x, y, Rgba::from_channels(1, 1, 1, 255)),
                    (r, g, b) => imgc.put_pixel(x, y, Rgba::from_channels(r, g, b, 255)),
                }
                pixels_notsame += 1;
            }
        }
    }

    (
        imgc,
        calc_percent_transparent(pixels_same, pixels_same + pixels_notsame),
    )
}

fn add2(image_base: DynamicImage, image_extra: &DynamicImage) -> (DynamicImage, u64) {

    let (w, h) = image_base.dimensions();
    let mut image_output = image::DynamicImage::new_rgb8(w, h);

    let mut pixels_transparent: u64 = 0;
    for y in 0..h {
        for x in 0..w {
            let pixel_a = image_base.get_pixel(x, y);

            match (pixel_a[0], pixel_a[1], pixel_a[2]) {
                (0, 0, 0) => {
                    let pixel_b = image_extra.get_pixel(x, y);
                    image_output.put_pixel(x, y, pixel_b);
                    if pixel_a == pixel_b {
                        pixels_transparent += 1;
                    }
                }
                (_, _, _) => image_output.put_pixel(x, y, image_base.get_pixel(x, y)),
            }
        }
    }

    return (
        image_output,
        calc_percent_transparent(pixels_transparent, (w * h * 3) as u64),
    );
}
