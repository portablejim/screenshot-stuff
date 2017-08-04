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
use std::char;
use std::io::Read;
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::ascii::AsciiExt;
use image::{GenericImage,Rgb,Rgba,Pixel,DynamicImage};
use std::io::Write;
use std::hash::BuildHasherDefault;
use std::collections::HashMap;
use twox_hash::XxHash;
use std::hash::Hasher;
use png::HasParameters;

extern crate imagefmt;
use imagefmt::{ColFmt, ColType};

fn main() {
    let args: Vec<String> = env::args().collect();

    let trns_black_transparent: [u8; 6] = [0, 0, 0, 0, 0 ,0];

    if args.len() == 2 {
        let image_hashes: HashMap<usize, String> = HashMap::new();
        //let mut image_hashes2: HashMap<u64, Vec<usize>> = HashMap::new();
        let mut image_hashes2: HashMap<u64, String> = HashMap::new();

        let mut timings_file: String = String::new();
        let timings_file_arg = args.get(1).expect("Error getting timings file argument");
        File::open(timings_file_arg).expect("No such file").read_to_string(&mut timings_file);
        let timings_dir = Path::new(args.get(1).expect("Error getting timings arg")).parent().unwrap_or(Path::new("."));
        println!("{:?}", timings_dir);
        let timings: Vec<Vec<String>> = serde_json::from_slice(timings_file.as_ref()).unwrap_or(vec![]);
        let mut timings_new: Vec<Vec<String>> = vec![];

        let images_path = timings_dir.join("images");
        if !images_path.is_dir() && fs::create_dir(&images_path).is_err() {
            println!("Error: Can't use 'images' directory");
            return;
        }

        let mut previous: Option<DynamicImage> = None;
        let mut entry_num: usize = 0;
        println!("{}", timings.len());
        for entry_num in 0..timings.len() {
            let entry = timings.get(entry_num).expect("Weird length for timings");
            if entry.len() >= 2 {
                let entry_image = timings_dir.join(entry.get(1).expect("No path supplied"));
                previous = match previous {
                    None => {
                        // First entry
                        match image::open(timings_dir.join(entry_image)) {
                            Ok(rgba_image_data) => {
                                let mut image_data_pre: DynamicImage = DynamicImage::ImageRgb8(rgba_image_data.to_rgb());
                                // Generate hash
                                let mut hasher = XxHash::default();
                                for pixel in rgba_image_data.raw_pixels() {
                                    hasher.write_u8(pixel);
                                }
                                let hash_value = hasher.finish();

                                let (w,h) = image_data_pre.dimensions();
                                for y in 0..h {
                                    for x in 0..w {
                                        let pixel = image_data_pre.get_pixel(x, y);
                                        match (pixel[0], pixel[1], pixel[2]) {
                                            (0,0,0) => image_data_pre.put_pixel(x, y, Rgba::from_channels(1,1,1,255)),
                                            _ => ()
                                        }
                                    }
                                }
                                let image_data = image_data_pre;

                                // Setup oxipng
                                let out_name = format!("slide{:03}.png", entry_num+1);
                                let mut oxioptions = oxipng::Options::from_preset(4);
                                oxioptions.interlace = Some(1);
                                oxioptions.verbosity = Some(0);
                                oxioptions.bit_depth_reduction = false;
                                oxioptions.color_type_reduction = false;
                                oxioptions.out_file = images_path.join(&out_name);
                                let out_relpath = match images_path.file_name().and_then(|n|n.to_str()) {
                                    Some(images_path_name) => format!("{}/{}", images_path_name, out_name),
                                    None => String::new()
                                };

                                println!("Hash: {}, seen: {}", hash_value, image_hashes2.contains_key(&hash_value));
                                image_hashes2.insert(hash_value, out_relpath.clone());

                                // Save png with oxipng
                                let mut image_vec: Vec<u8> = Vec::new();
                                let (img_width, img_height) = image_data.dimensions();
                                println!("Pixels: {} vs {} vs {}", img_width * img_height*3, &image_data.raw_pixels().len(), img_width*img_height*4);
                                {
                                    let mut img_encoder = png::Encoder::new(&mut image_vec, img_width, img_height);
                                    img_encoder.set(png::ColorType::RGB).set(png::BitDepth::Eight);
                                    let mut img_writer = img_encoder.write_header().expect("Problem writing headers");
                                    img_writer.write_chunk(png::chunk::tRNS, &trns_black_transparent);
                                    img_writer.write_image_data(&image_data.raw_pixels());
                                }

                                println!("out-first: {}", oxioptions.out_file.to_str().unwrap_or("(none)"));
                                let oxi_output = oxipng::optimize_from_memory(&image_vec, &oxioptions).expect("Error creating compressed image_data");
                                File::create(images_path.join(&out_name)).expect("Error writing").write(&oxi_output);

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
                                println!("Hash: {}, seen: {}", hash_value, image_hashes2.contains_key(&hash_value));

                                let (image_diff, diff_percent) = diff2(previous_entry, &image_data);

                                if image_hashes2.contains_key(&hash_value) {
                                    let other_image_path = &image_hashes2[&hash_value];
                                    match image::open(timings_dir.join(other_image_path)) {
                                        Ok(other_image_data_pre) => {
                                            let other_image_data = image::ImageRgb8(other_image_data_pre.to_rgb());
                                            let (image_addition, diff_percent) = add2(other_image_data, &image_data);

                                            // Setup oxipng
                                            let mut oxioptions = oxipng::Options::from_preset(4);
                                            //oxioptions.interlace = Some(1);
                                            oxioptions.verbosity = Some(0);
                                            oxioptions.bit_depth_reduction = false;
                                            oxioptions.color_type_reduction = false;
                                            oxioptions.out_file = timings_dir.join(other_image_path);
                                            let out_relpath = other_image_path;

                                            // Save png with oxipng
                                            let mut image_vec: Vec<u8> = Vec::new();
                                            let (img_width, img_height) = image_addition.dimensions();
                                            println!("Pixels: {} vs {} vs {}", img_width * img_height*3, &image_addition.raw_pixels().len(), img_width*img_height*4);
                                            {
                                                let mut img_encoder = png::Encoder::new(&mut image_vec, img_width, img_height);
                                                img_encoder.set(png::ColorType::RGB).set(png::BitDepth::Eight);
                                                let mut img_writer = img_encoder.write_header().expect("Problem writing headers");
                                                img_writer.write_chunk(png::chunk::tRNS, &trns_black_transparent);
                                                img_writer.write_image_data(&image_addition.raw_pixels());
                                            }

                                            println!("out-add: {}", oxioptions.out_file.to_str().unwrap_or("(none)"));
                                            let oxi_output = oxipng::optimize_from_memory(&image_vec, &oxioptions).expect("Error creating compressed image_data");
                                            File::create(timings_dir.join(&other_image_path)).expect("Error writing").write(&oxi_output);

                                            let mut entry_new: Vec<String> = entry.clone();
                                            entry_new[1] = out_relpath.clone();
                                            timings_new.push(entry_new);

                                            Some(image_data)
                                        },
                                        Err(e) => {
                                            eprintln!("Error when reading old image: {:?}", e);
                                            None
                                        }
                                    }
                                } else {
                                    // Setup oxipng
                                    let out_name = format!("slide{:03}.png", entry_num + 1);
                                    let mut oxioptions = oxipng::Options::from_preset(4);
                                    //oxioptions.interlace = Some(1);
                                    oxioptions.verbosity = Some(0);
                                    oxioptions.out_file = images_path.join(&out_name);
                                    oxioptions.bit_depth_reduction = false;
                                    oxioptions.color_type_reduction = false;
                                    let out_relpath = match images_path.file_name().and_then(|n| n.to_str()) {
                                        Some(images_path_name) => format!("{}/{}", images_path_name, out_name),
                                        None => String::new()
                                    };
                                    image_hashes2.insert(hash_value, out_relpath.clone());

                                    // Save png with oxipng
                                    let mut image_vec: Vec<u8> = Vec::new();
                                    let (img_width, img_height) = image_diff.dimensions();
                                    println!("Pixels: {} vs {} vs {}", img_width * img_height*3, &image_diff.raw_pixels().len(), img_width*img_height*4);
                                    {
                                        let mut img_encoder = png::Encoder::new(&mut image_vec, img_width, img_height);
                                        img_encoder.set(png::ColorType::RGB).set(png::BitDepth::Eight);
                                        let mut img_writer = img_encoder.write_header().expect("Problem writing headers");
                                        img_writer.write_chunk(png::chunk::tRNS, &trns_black_transparent);
                                        img_writer.write_image_data(&image_diff.raw_pixels());
                                    }

                                    println!("out-diff: {}", oxioptions.out_file.to_str().unwrap_or("(none)"));
                                    let oxi_output = oxipng::optimize_from_memory(&image_vec, &oxioptions).expect("Error creating compressed image_data");
                                    File::create(images_path.join(&out_name)).expect("Error writing").write(&oxi_output);

                                    let mut entry_new: Vec<String> = entry.clone();
                                    entry_new[1] = out_relpath;
                                    timings_new.push(entry_new);

                                    Some(image_data)
                                }
                                }
                                Err(e) => {
                                eprintln ! ("Error when reading {:?}", e);
                                None
                                }
                            }
                        }
                    }
                }
            }

        println!("New json: {:?}", timings_new);
        let timings_new_string = serde_json::to_string(&timings_new).expect("Error serialising new timings");
        let mut rewrite_options = OpenOptions::new();
        rewrite_options.write(true);
        rewrite_options.truncate(true);
        match rewrite_options.open(timings_file_arg).and_then(|mut f|f.write_all(&timings_new_string.as_bytes())) {
            Ok(_) => (),
            Err(e) => eprintln!("Error writing new json file: {:?}", e)
        }
    }
    return;
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
                                (0,0,0) => imgc.put_pixel(x, y, Rgba::from_channels(1,1,1, 255)),
                                (r,g,b) => imgc.put_pixel(x, y, Rgba::from_channels(r,g,b, 255))
                            }
                            pixels_notsame += 1;
                        }
                    }
                }
                let pixels_same_percent = (pixels_same * 100) / (pixels_notsame + pixels_same);

    (imgc, pixels_same_percent)
}

fn add2(image_base: DynamicImage, extra: &DynamicImage) -> (DynamicImage, u64) {
    // TODO: Make it work properly
    return (image_base, 50);
}
