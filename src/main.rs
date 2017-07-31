extern crate image;
extern crate scrap;
extern crate time;
extern crate twox_hash;
extern crate dxgcap;

use image::{ImageBuffer, Rgba};
use scrap::{Capturer, Display};
use std::io::ErrorKind::WouldBlock;
use std::path::Path;
use std::thread;
use std::time::Duration;
use std::sync::mpsc;
use std::sync::mpsc::{Sender, Receiver};
use twox_hash::XxHash;
use std::hash::BuildHasherDefault;
use std::hash::Hasher;
use dxgcap::DXGIManager;

struct FrameInfo {
    time: u64,
    w: usize,
    h: usize,
    frame: Vec<u8>,
}

fn main() {
    let one_second = Duration::new(1, 0);
    let one_frame = one_second / 20;

    let mut displays = Display::all().expect("Couldn't find displays.");
    let display = displays.remove(0);
    
    let mut capturer = Capturer::new(display).expect("Couldn't begin capture.");
    let (w, h) = (capturer.width(), capturer.height());

    let pixels = w * h * 4;
    //let (w2, h2) = (&w, *h);

    // Setup threads
    let (tx_all, rx_all): (Sender<FrameInfo>, Receiver<FrameInfo>) = mpsc::channel();
    let (tx_filtered, rx_filtered): (Sender<FrameInfo>, Receiver<FrameInfo>) = mpsc::channel();
    let handle = thread::spawn(move || {
        let mut i = 0;

        let mut last_saved: Vec<u8> = vec![0; pixels];

        for frameinfo in rx_all {

            let frametime = (frameinfo.time as f64) / 1_000.0;
            let w = frameinfo.w;
            let h = frameinfo.h;
            let buffer = frameinfo.frame;

            if last_saved != buffer {
                // PistonDevelopers/image doesn't support ARGB images yet.
                // But they will soon!
                let mut bitflipped = Vec::with_capacity(w * h * 4);
                for pixel in buffer.chunks(4) {
                    let (b, g, r, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
                    bitflipped.extend_from_slice(&[r, g, b, a]);
                }

                let mut hash: XxHash = XxHash::default();
                hash.write(&buffer);
                let hash_value = hash.finish();

                let pathname = format!("screenshot{:X}.png", hash_value);
                let path = Path::new(&pathname);

                if path.exists() {
                    println!("Image already saved to `{}` @ {}.", pathname, frametime);
                }
                else {
                    let image: ImageBuffer<Rgba<u8>, _> =
                        ImageBuffer::from_raw(w as u32, h as u32, bitflipped)
                            .expect("Couldn't convert frame into image buffer.");

                    image.save(&path).expect(&format!(
                        "Couldn't save image to `screenshot{}.png`.",
                        i
                    ));
                    println!("Image saved to `{}` @ {}.", pathname, frametime);
                    i += 1;
                }
                last_saved = buffer;

            }
            else {
                println!("Ignored frame");
            }
        }
    });

    {

        let tx1 = tx_all;

        let base_epoch = time::precise_time_ns();

        for _ in 0..200 {
            loop {
                let buffer = match capturer.frame() {
                    Ok(buffer) => buffer,
                    Err(error) => {
                        if error.kind() == WouldBlock {
                            // Keep spinning.
                            thread::sleep(one_frame);
                            continue;
                        } else {
                            panic!("Capture error: {}", error);
                        }
                    }
                };

                //println!("Captured! Saving...");

                tx1.send(FrameInfo {
                    time: (time::precise_time_ns() - base_epoch) / 1_000_000,
                    w: w,
                    h: h,
                    frame: (*buffer).iter().cloned().collect(),
                }).expect("Error sending raw image data.");

                break;
            }

        }
    }

    handle.join().expect("Error finishing up.");
}
