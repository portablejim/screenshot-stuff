#![feature(iterator_step_by)]

extern crate image;
extern crate rayon;

use std::env;
use std::fs;
use std::fs::DirEntry;
use std::io;
use std::path::Path;
use image::{DynamicImage, GenericImage};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;

use rayon::prelude::*;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() >= 2 {
        let directory = &args[1].to_string();
        let images = Arc::new(fetch_images(directory).expect("Failed to get images"));
        let dupes = find_dupe_indexes(&images);

        for (i_a, i_b) in dupes {
            let img_path_a = &images[i_a].path;
            let img_path_b = &images[i_b].path;
            println!("Remove {}", img_path_b);
            println!("Link {} to {}", img_path_a, img_path_b);
        }
    }
}

fn find_dupe_indexes(images: &Vec<ImageInfo>) -> Vec<(usize, usize)> {
    let (work_tx, work_rx_raw) = channel();
    let (results_tx_original, results_rx) = channel();
    let work_rx = Arc::new(Mutex::new(work_rx_raw));

    let return_val;

    for i in 0..(0 + images.len() / 1 - 1) {
        for j in i + 1..images.len() {
            work_tx.send((i, j)).ok();
        }
    }
    {
        let mut results_tx = results_tx_original;
        let mut threads = Vec::new();
        for _ in 0..6 {
            let own_work_rx = work_rx.clone();
            let own_results_tx = results_tx.clone();
            let images = images.clone();
            threads.push(thread::spawn(move || {
                loop {
                    let (n_a, n_b) = {
                        match own_work_rx.lock() {
                            Ok(rx) => {
                                match rx.try_recv() {
                                    Ok((a, b)) => (a, b),
                                    _ => break,
                                }
                            }
                            _ => continue,
                        }
                    };
                    let image_a: &ImageInfo = &images[n_a];
                    let image_b: &ImageInfo = &images[n_b];

                    if image_a.pixels.len() != image_b.pixels.len() {
                        continue;
                    }

                    let distance_cutoff = 8;
                    let pixel_cutoff = 4;

                    let mut significantly_different: u64 = 0;
                    for n in (0..image_a.pixels.len()).step_by(3) {
                        let diff_r = (image_a.pixels[n] as i32 - image_b.pixels[n] as i32).abs() >
                            distance_cutoff;
                        let diff_g = (image_a.pixels[n + 1] as i32 -
                                          image_b.pixels[n + 1] as i32)
                            .abs() > distance_cutoff;
                        let diff_b = (image_a.pixels[n + 2] as i32 -
                                          image_b.pixels[n + 2] as i32)
                            .abs() > distance_cutoff;
                        if diff_r && diff_g && diff_b {
                            significantly_different += 1;
                        }
                        // +2 Just to make sure
                        if significantly_different > pixel_cutoff + 2 {
                            // Already too different. Save cycles
                            break;
                        }
                    }
                    // Pixels that are significatly different.
                    // Should probably be 0, but to give a tiny bit of leeway.
                    if significantly_different <= 4 {
                        own_results_tx.send((n_a, n_b)).ok();
                    }
                }
            }));
        }
        for thread in threads {
            match thread.join() {
                Err(e) => eprintln!("Error joining thread: {:?}", e),
                _ => (),
            }
        }
    }
    return_val = results_rx.iter().collect();

    return return_val;

}

#[derive(Clone)]
struct ImageInfo {
    path: String,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

fn is_image(file_name: String) -> bool {
    //let file_name: &str = &file_name;
    let file_types = vec![".png", ".jpg"];
    for t in file_types {
        if file_name.ends_with(t) {
            return true;
        }
    }
    return false;
}

fn fetch_images(folder_name: &String) -> Result<Vec<ImageInfo>, io::Error> {
    let entries = fs::read_dir(folder_name)?;
    let file_list: Vec<DirEntry> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().and_then(|t| Ok(t.is_file())).unwrap_or(false)
        })
        .filter(|e| {
            e.file_name()
                .into_string()
                .and_then(|n| Ok(is_image(n)))
                .unwrap_or(false)
        })
        .collect();
    let output_vec: Vec<ImageInfo> = file_list
        .into_par_iter()
        .filter_map(|p| {
            image::open(&p.path()).ok().and_then(
                |img| Some((p.path(), img)),
            )
        })
        .map(|(path, img)| {
            let (w, h) = img.dimensions();
            ImageInfo {
                path: path.to_str().unwrap_or("").to_owned(),
                width: w,
                height: h,
                pixels: DynamicImage::ImageRgb8(img.to_rgb()).raw_pixels(),
            }
        })
        .collect();

    Ok(output_vec)
}
