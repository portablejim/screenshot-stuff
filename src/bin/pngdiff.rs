#![feature(slice_patterns)]

extern crate image;
extern crate serde_json;
extern crate oxipng;
//extern crate imagequant;
extern crate png;
extern crate twox_hash;

use std::{env, fs, thread};
use std::path::Path;
use std::path::PathBuf;
use std::fs::File;
use std::io::Read;
use std::fs::OpenOptions;
use image::{ImageFormat, GenericImage, Rgba, Pixel, DynamicImage};
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

        let timings_file_arg = args.get(1).expect("Error getting timings file argument");
        let (timings_dir, timings) = read_timings(timings_file_arg);
        println!("{:?}", timings_dir);
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
                        &mut timings_new,
                        &timings_dir,
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

fn read_timings(path: &String) -> (PathBuf, Vec<Vec<String>>) {
    let mut timings_file: String = String::new();
    File::open(path)
        .expect("No such file")
        .read_to_string(&mut timings_file)
        .expect("Error reading timings file");
    let timings_dir = Path::new(path).parent().unwrap_or(Path::new("."));
    let timings: Vec<Vec<String>> = serde_json::from_slice(timings_file.as_ref()).unwrap_or(vec![]);

    (timings_dir.to_owned(), timings)
}

fn handle_timings_entry(
    entry_num: usize,
    entry: &Vec<String>,
    previous: Option<DynamicImage>,
    image_hashes: &mut HashMap<u64, String>,
    timings_new: &mut Vec<Vec<String>>,
    timings_dir: &Path,
    images_path: &Path,
) -> Result<(Option<DynamicImage>, Vec<String>), (String, Option<DynamicImage>)> {
    let entry_image = match entry.as_slice() {
        &[_, ref image, _..] => timings_dir.join(image),
        _ => return Err((format!("Error with entry {}", entry_num), previous)),
    };
    let image_data = match image::open(timings_dir.join(&entry_image)) {
        Ok(data) => data,
        _ => return Err((format!("Error loading img: {:?}", entry_image), previous)),
    };

    // Generate hash
    let hasher_thread = img_gen_hash(&image_data);

    let jpg_thread = img_gen_jpg(&image_data);

    let rgb_image_data = to_rgb_image(&image_data);
    let (image_diff, diff_percent) = match &previous {
        &None => (posterize_lite(rgb_image_data), 0),
        &Some(ref previous_entry) => diff2(&previous_entry, &rgb_image_data),
    };

    let hash_value = match hasher_thread.join() {
        Ok(h) => h,
        _ => return Err((format!("Error generating hash"), previous)),
    };

    let out_name = format!("slide{:03}.png", entry_num + 1);
    let (name_post_hash, image_post_hash, post_hash_percent, hash_matched) =
        if image_hashes.contains_key(&hash_value) {
            let other_image_path = &image_hashes[&hash_value];
            match image::open(timings_dir.join(other_image_path)).map(|i| ImageRgb8(i.to_rgb())) {
                Ok(other_image_data) => {
                    let (a, b) = add2(other_image_data, &image_diff);
                    (other_image_path.to_string(), a, b, true)
                }
                _ => (out_name, image_diff, diff_percent, false),
            }
        } else {
            (out_name, image_diff, diff_percent, false)
        };
    let out_relpath = match images_path.file_name().and_then(|n| n.to_str()) {
        Some(images_path_name) => format!("{}/{}", images_path_name, name_post_hash),
        None => String::new(),
    };
    let mut save_filename = images_path.join(name_post_hash);
    let image_png = save_image(&save_filename, &image_post_hash, post_hash_percent);
    let image_smaller = match jpg_thread.join() {
        Ok(Some(jpg_data)) => {
            let jpg_len = jpg_data.len();
            let png_len = image_png.len();

            if jpg_len * 5 < png_len * 2 {
                println!("JPEG is smaller");
                let old_save_filename = save_filename.clone();
                save_filename.set_extension("jpg");
                if hash_matched && old_save_filename != save_filename {
                    for e in timings_new.iter_mut() {
                        if e.len() >= 2 && Some(&*e[1]).eq(&old_save_filename.to_str()) {
                            e[1] = save_filename.to_string_lossy().to_string();
                            fs::remove_file(&old_save_filename).unwrap_or_else(|_| {
                                eprintln!("Error removing file")
                            });
                        }
                    }
                }
                jpg_data
            } else {
                println!("PNG is smaller");
                image_png
            }
        }
        _ => image_png,
    };

    match File::create(&save_filename)
        .expect(&format!("Error writing final image: {:?}", &save_filename))
        .write(&image_smaller) {
        Ok(_) => (),
        Err(e) => eprintln!("Error writing optimised image: {:?}", e),
    }
    match save_filename.clone().to_str() {
        Some(file_name) => {
            image_hashes
                .insert(hash_value, file_name.to_owned())
                .is_some()
        }
        _ => false,
    };

    let mut entry_new: Vec<String> = entry.clone();
    entry_new[1] = out_relpath.clone();
    //timings_new.push(entry_new);

    Ok((Some(image_data), entry_new))
}

fn img_gen_hash(image_in: &DynamicImage) -> thread::JoinHandle<u64> {
    let image = image_in.clone();
    thread::spawn(move || {
        let mut hasher = XxHash::default();
        for pixel in image.raw_pixels() {
            hasher.write_u8(pixel);
        }
        let hash_value = hasher.finish();
        println!(
            "Hash: {}",
            hash_value,
        );
        hash_value
    })
}

fn img_gen_jpg(image_in: &DynamicImage) -> thread::JoinHandle<Option<Vec<u8>>> {
    let image = image_in.clone();
    thread::spawn(move || {
        let mut image_data: Vec<u8> = Vec::new();
        match image.save(&mut image_data, ImageFormat::JPEG) {
            Ok(_) => Some(image_data),
            _ => None,
        }
    })
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

fn save_image(out_path: &Path, input_image: &DynamicImage, percent_transparent: u64) -> Vec<u8> {
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
            }
        }
    }

    let oxi_output = oxipng::optimize_from_memory(&image_vec, &oxioptions)
        .expect("Error creating compressed image_data");

    oxi_output
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
