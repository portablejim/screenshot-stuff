#![feature(slice_patterns)]

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
            match timings.get(entry_num) {
                Some(entry) if entry.len() >= 2 => {
                    eprintln!("Entry {}", entry_num);
                    previous = match handle_timings_entry(
                        entry_num,
                        entry,
                        previous,
                        &mut image_hashes,
                        timings_dir,
                        &images_path,
                    ) {
                        Ok((new_previous, new_entry)) => {
                            timings_new.push(new_entry);
                            new_previous
                        }
                        Err((e, old_previous)) => {
                            eprintln!("{}", e);
                            old_previous
                        }
                    }
                }
                Some(entry) => eprintln!("Entry {} length wrong: {:?}", entry_num, entry),
                None => eprintln!("Error on entry {}", entry_num),
            }
        }

        println!("Old json: {:?}", timings);
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

fn handle_timings_entry(
    entry_num: usize,
    entry: &Vec<String>,
    previous: Option<DynamicImage>,
    image_hashes: &mut HashMap<u64, String>,
    timings_dir: &Path,
    images_path: &Path,
) -> Result<(Option<DynamicImage>, Vec<String>), (String, Option<DynamicImage>)> {
    //let entry_image = timings_dir.join(entry[1]);
    let entry_image = match entry.as_slice() {
        &[_, ref image, _..] => timings_dir.join(image),
        _ => {
            return Err((
                format!("Error extracting config for item {}", entry_num),
                previous,
            ))
        }
    };
    let image_data = match image::open(timings_dir.join(&entry_image)) {
        Ok(data) => data,
        _ => {
            return Err((
                format!("Error loading image: {:?}", entry_image),
                previous,
            ))
        }
    };

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

    let rgb_image_data = to_rgb_image(&image_data);
    let (image_diff, diff_percent) = match previous {
        None => (posterize_lite(rgb_image_data), 0),
        Some(previous_entry) => diff2(&previous_entry, &rgb_image_data),
    };

    let out_name = format!("slide{:03}.png", entry_num + 1);
    let (name_post_hash, image_post_hash, post_hash_percent) =
        if image_hashes.contains_key(&hash_value) {
            let other_image_path = &image_hashes[&hash_value];
            match image::open(timings_dir.join(other_image_path)).map(|i| ImageRgb8(i.to_rgb())) {
                Ok(other_image_data) => {
                    let (a, b) = add2(other_image_data, &image_diff);
                    (other_image_path.to_string(), a, b)
                }
                _ => (out_name, image_diff, diff_percent),
            }
        } else {
            (out_name, image_diff, diff_percent)
        };
    let out_relpath = match images_path.file_name().and_then(|n| n.to_str()) {
        Some(images_path_name) => format!("{}/{}", images_path_name, name_post_hash),
        None => String::new(),
    };
    if image_hashes.contains_key(&hash_value) {
        image_hashes.insert(hash_value, out_relpath.clone());
    }
    save_image(
        &images_path.join(name_post_hash),
        &image_post_hash,
        post_hash_percent,
    );

    let mut entry_new: Vec<String> = entry.clone();
    entry_new[1] = out_relpath.clone();
    //timings_new.push(entry_new);

    Ok((Some(image_data), entry_new))
}

fn posterize_lite(image_data: DynamicImage) -> DynamicImage {
    let mut image_data_mut = image_data;
    let (w, h) = image_data_mut.dimensions();
    for y in 0..h {
        for x in 0..w {
            let pixel = image_data_mut.get_pixel(x, y);
            match (pixel[0], pixel[1], pixel[2]) {
                (0, 0, 0) => image_data_mut.put_pixel(x, y, Rgba::from_channels(1, 1, 1, 255)),
                _ => (),
            }
        }
    }
    image_data_mut
}

fn to_rgb_image(input_image: &DynamicImage) -> DynamicImage {
    match input_image {
        &image::ImageRgb8(_) => input_image.clone(),
        _ => ImageRgb8(input_image.to_rgb().clone()),

    }
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
        if percent_transparent != 0 {
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
