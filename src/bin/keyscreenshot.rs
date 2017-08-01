extern crate ctrlc;
extern crate image;
extern crate scrap;
extern crate time;
extern crate dxgcap;
extern crate serde;
extern crate serde_json;

use image::{ImageBuffer, Rgba};
use std::path::Path;
use std::thread;
use std::time::Duration;
use std::sync::mpsc;
use std::sync::mpsc::{Sender, Receiver};
use dxgcap::DXGIManager;
use dxgcap::BGRA8;
use dxgcap::CaptureError;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::fs::File;

#[derive(Clone)]
struct FrameInfo {
    time: u64,
    w: usize,
    h: usize,
    frame: Vec<BGRA8>,
}

fn main() {
    let one_second = Duration::new(1, 0);
    let one_frame = one_second / 5;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || { r.store(false, Ordering::SeqCst); })
        .expect("Error setting Ctrl-C handler");

    let mut manager = DXGIManager::new(200).expect("Unable to make manager.");
    manager.set_capture_source_index(0);
    //manager.acquire_output_duplication();

    //let pixels = w * h * 4;
    //let (w2, h2) = (&w, *h);

    // Setup threads
    let (tx_all, rx_all): (Sender<FrameInfo>, Receiver<FrameInfo>) = mpsc::channel();
    let handle = thread::spawn(move || {
        let mut i = 0;

        let mut last_saved: Option<Vec<BGRA8>> = None;

        let mut ignored = false;
        //ctrlc::set_handler(move || {} ).expect("Error setting ctrlc handler");

        let mut timings: Vec<Vec<String>> = vec![];

        for frameinfo in rx_all {

            let frametime = (frameinfo.time as f64) / 1_000.0;
            let w = frameinfo.w;
            let h = frameinfo.h;
            let buffer = frameinfo.frame;

            last_saved = match last_saved {
                None => Some(buffer),
                Some(last_saved) => {
                    if last_saved == buffer {
                        println!("Ignored frame");
                        Some(last_saved)
                    } else {
                        let mut bitflipped = Vec::with_capacity(w * h * 4);
                        for pixel in &buffer {
                            //let (b, g, r, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
                            //bitflipped.extend_from_slice(&[r, g, b, a]);
                            bitflipped.extend_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a])
                        }

                        let pathname = format!("screenshot{:03}.png", i);
                        let path = Path::new(&pathname);

                        let image: ImageBuffer<Rgba<u8>, _> =
                            ImageBuffer::from_raw(w as u32, h as u32, bitflipped)
                                .expect("Couldn't convert frame into image buffer.");

                        image.save(&path).expect(&format!(
                            "Couldn't save image to `screenshot{}.png`.",
                            i
                        ));

                        let frametime_hours = frametime as u32 / 3600;
                        let frametime_minutes = (frametime as u32 % 3600) / 60;
                        let frametime_seconds = frametime % 60 as f64;
                        let frametime_string = format!(
                            "{:02}:{:02}:{:06.3}",
                            frametime_hours,
                            frametime_minutes,
                            frametime_seconds
                        );
                        println!(
                            "Image saved to `{}` @ {} - {} ",
                            pathname,
                            frametime_string,
                            frametime
                        );

                        timings.push(vec![frametime_string, pathname.clone()]);

                        i += 1;

                        Some(buffer)
                    }
                }
            };
        }
        println!("Finishing up there...");
        let mut timings_file = File::create("timings.json");
        match timings_file {
            Ok(f) => serde_json::to_writer(f, &timings).expect("Error writing timings file"),
            Err(e) => println!("Error creating timings file"),
        };
        //buffer.write(b"some bytes")?;
        println!("Finished up there...");
    });

    {

        let tx1 = tx_all;

        let base_epoch = time::precise_time_ns();

        let mut frameinfo_last: Option<FrameInfo> = None;
        for _ in 0..200 {
            if !running.load(Ordering::SeqCst) {
                break;
            }
            while running.load(Ordering::SeqCst) {
                let (buffer, w, h) = match manager.capture_frame() {
                    Ok((buffer, (w, h))) => (buffer, w, h),
                    Err(CaptureError::Timeout) => {
                        match frameinfo_last.clone() {
                            None => continue,
                            Some(frameinfo) => {
                                tx1.send(frameinfo).expect("Error sending raw image data.");
                                frameinfo_last = None;
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        /*if error.kind() == WouldBlock {
                            // Keep spinning.
                            thread::sleep(one_frame);
                            continue;
                        } else {
                            panic!("Capture error: {}", error);
                        }*/
                        println!("Error: {:?} -> Sleeping for {:?}", error, one_frame);
                        thread::sleep(one_frame);
                        continue;
                    }
                };

                frameinfo_last = Some(FrameInfo {
                    time: (time::precise_time_ns() - base_epoch) / 1_000_000,
                    w: w,
                    h: h,
                    frame: buffer,
                });

                //println!("Captured! Saving...");

                /*tx1.send(FrameInfo {
                    time: (time::precise_time_ns() - base_epoch) / 1_000_000,
                    w: w,
                    h: h,
                    frame: (*buffer).iter().cloned().collect(),
                }).expect("Error sending raw image data.");*/

                continue;
            }

        }
    }
    println!("Finishing up here...");
    handle.join().expect("Error finishing up.");
    println!("Finished")
}
