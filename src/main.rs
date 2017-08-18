extern crate gtk;
extern crate cairo;
extern crate serial;
extern crate serial_core;
extern crate gdk;
extern crate gdk_pixbuf;

use std::cell::Cell;
use std::rc::Rc;

use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use std::thread;

use std::time::Duration;

use std::io::Read;

use std::str;

use std::f64::consts::PI;


use gtk::prelude::*;

use gtk::{DrawingArea, Window, WindowType};

use gdk::prelude::ContextExt;

use gdk_pixbuf::Pixbuf;

use cairo::enums::{FontSlant, FontWeight};

use serial_core::SerialDevice;


// Fair map

const IMG_SRC: &str = "/home/pi/aquabot-locator/fair_map.jpg";

const START_LAT: f64 = 40.416623;
const START_LONG: f64 = -74.883628;
const END_LAT: f64 = 40.413900;
const END_LONG: f64 = -74.880168;
const START_X: f64 = 0.0;
const START_Y: f64 = 0.0;
const END_X: f64 = 808.0;
const END_Y: f64 = 831.0;


// Tim's House
/*
const IMG_SRC: &str = "tim_map.png";

const START_LAT: f64 = 40.672233;
const START_LONG: f64 = -74.841474;
const END_LAT: f64 = 40.671406;
const END_LONG: f64 = -74.839740;

const START_X: f64 = 0.0;
const START_Y: f64 = 0.0;
const END_X: f64 = 955.0;
const END_Y: f64 = 598.0;
*/


const LONG_DIST: f64 = START_LONG - END_LONG;
const LAT_DIST: f64 = START_LAT - END_LAT;

const DIST_X: f64 = START_X - END_X;
const DIST_Y: f64 = START_Y - END_Y;

// make moving clones into closures more convenient
macro_rules! clone {
    (@param _) => ( _ );
    (@param $x:ident) => ( $x );
    ($($n:ident),+ => move || $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move || $body
        }
    );
    ($($n:ident),+ => move |$($p:tt),+| $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move |$(clone!(@param $p),)+| $body
        }
    );
}

#[derive(Default, Copy, Clone, Debug)]
struct AquabotData {
    msg_num: u32,
    fix: bool,
    satelites: u32,
    latitude: f64,
    longitude: f64,
    signal_strength: i32,
}

fn main() {

    let aquabot_cell: Rc<Cell<AquabotData>> = Rc::new(Cell::default());

    if gtk::init().is_err() {
        println!("Failed to initialize GTK.");
        return;
    }

    let window = Window::new(WindowType::Toplevel);
    window.set_title("Aquabot Locator");
    window.set_default_size(1920, 1080);

    let drawing_area = Box::new(DrawingArea::new)();

    let pixbuf = Pixbuf::new_from_file(IMG_SRC).unwrap();

    drawing_area.connect_draw(clone!(aquabot_cell => move |widget, cr| {

        let aquabot_data = aquabot_cell.get();

        let img_ratio = pixbuf.get_height() as f64 / pixbuf.get_width() as f64;
        let screen_ratio = widget.get_allocated_height() as f64 / widget.get_allocated_width() as f64;

        let scale = if img_ratio > screen_ratio {
            widget.get_allocated_height() as f64 / pixbuf.get_height() as f64
        } else {
            widget.get_allocated_width() as f64 / pixbuf.get_width() as f64
        };

        cr.scale(scale, scale);

        let tx = (widget.get_allocated_width() as f64 - pixbuf.get_width() as f64 * scale) / 2.0;
        let ty = (widget.get_allocated_height() as f64 - pixbuf.get_height() as f64 * scale) / 2.0;

        println!("({}, {})", tx, ty);

        cr.translate(tx, ty);

        cr.set_source_pixbuf(&pixbuf, 0.0, 0.0);

        cr.paint();
        cr.fill();

        let aquabot_x = ((aquabot_data.longitude - START_LONG) / LONG_DIST) * DIST_X + START_X;
        let aquabot_y = ((aquabot_data.latitude - START_LAT) / LAT_DIST) * DIST_Y + START_Y;

        cr.set_source_rgb(0.0, 0.0, 0.7);

        cr.arc(aquabot_x, aquabot_y, 15.0, 0.0, 2.0*PI);

        cr.move_to(aquabot_x - 20.0, aquabot_y);
        cr.line_to(aquabot_x + 20.0, aquabot_y);
        cr.move_to(aquabot_x, aquabot_y - 20.0);
        cr.line_to(aquabot_x, aquabot_y + 20.0);
        cr.stroke();

        cr.translate(-tx, -ty);

        cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Normal);
        cr.set_font_size(20.0);

        cr.move_to(10.0, 30.0);

        if aquabot_data.fix {
            cr.set_source_rgb(0.0, 0.7, 0.0);
            cr.show_text("GPS Fix!");
        } else {
            cr.set_source_rgb(0.1, 0.0, 0.0);
            cr.show_text("No GPS Fix");
        }

        cr.set_source_rgb(0.0, 0.0, 0.0);

        cr.move_to(10.0, 60.0);
        cr.show_text(&format!("GPS Satelites: {}", aquabot_data.satelites));

        cr.move_to(10.0, 90.0);
        cr.show_text(&format!("GPS Latitude: {}", aquabot_data.latitude));

        cr.move_to(10.0, 120.0);
        cr.show_text(&format!("GPS Longitude: {}", aquabot_data.longitude));

        cr.move_to(10.0, 150.0);
        cr.show_text(&format!("Radio Strength: {}", aquabot_data.signal_strength));

        Inhibit(false)
    }));

    window.add(&drawing_area);
    window.show_all();

    let (tx, rx): (Sender<AquabotData>, Receiver<AquabotData>) = mpsc::channel();

    thread::spawn(move || {
        let mut serial_port = serial::open("/dev/ttyACM0").unwrap();

        if let Err(err) = serial_port.set_timeout(Duration::from_secs(2)) {
            println!("Could not set timeout on serial port: {:?}", err);
        }

        let mut buf: Vec<u8> = Vec::new();

        for byte in serial_port.bytes() {
            match byte {
                Ok(byte) => {
                    if byte == '\n' as u8 {
                        // Parse the string
                        if let Some(aquabot_data) = parse_buf(&buf) {
                            match tx.send(aquabot_data) {
                                Ok(_) => println!("Send successful"),
                                Err(err) => println!("Error sending! {:?}", err),
                            };
                        }
                        buf.clear();
                    } else {
                        buf.push(byte);
                    }
                }
                Err(err) => println!("Byte error! {:?}", err),
            }
        }
    });

    gtk::timeout_add(100, move || {
        match rx.try_recv() {
            Ok(new_aquabot_data) => {
                aquabot_cell.set(new_aquabot_data);
                drawing_area.queue_draw();
            }
            Err(err) => {
                if err != mpsc::TryRecvError::Empty {
                    println!("Error reciving: {:?}", err)
                }
            }
        }
        Continue(true)
    });

    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });

    gtk::main();
}

fn parse_buf(bytes: &[u8]) -> Option<AquabotData> {
    let mut aquabot_data: AquabotData = Default::default();
    let mut i = 0;
    for part in bytes.split(|b| *b == ':' as u8) {
        if let Ok(string) = str::from_utf8(&part) {
            let str_string = string.to_string();
            let trimed_string = str_string.trim();
            match i {
                0 => {
                    match trimed_string.parse::<u32>() {
                        Ok(num) => {
                            println!("Message Number: {:?}", num);
                            aquabot_data.msg_num = num;
                        }
                        Err(num) => {
                            println!("Err Message Number: {}", num);
                            return None;
                        }
                    }
                }
                1 => {
                    match trimed_string.parse::<u32>() {
                        Ok(num) => {
                            println!("Fix: {:?}", if num == 0 { false } else { true });
                            aquabot_data.fix = if num == 0 { false } else { true };
                        }
                        Err(num) => {
                            println!("Err Fix: {}", num);
                            return None;
                        }
                    }
                }
                2 => {
                    match trimed_string.parse::<u32>() {
                        Ok(num) => {
                            println!("Satelites: {:?}", num);
                            aquabot_data.satelites = num;
                        }
                        Err(num) => {
                            println!("Err Satelites: {}", num);
                            return None;
                        }
                    }
                }
                3 => {
                    match trimed_string.parse::<f64>() {
                        Ok(num) => {
                            println!("Latitude: {:?}", num);
                            aquabot_data.latitude = num;
                        }
                        Err(num) => {
                            println!("Err Latitude: {}", num);
                            return None;
                        }
                    }
                }
                4 => {
                    match trimed_string.parse::<f64>() {
                        Ok(num) => {
                            println!("Longitude: {:?}", num);
                            aquabot_data.longitude = num;
                        }
                        Err(num) => {
                            println!("Err Longitude: {}", num);
                            return None;
                        }
                    }
                }
                5 => {
                    match trimed_string.parse::<i32>() {
                        Ok(num) => {
                            println!("Signal Strength: {:?}", num);
                            aquabot_data.signal_strength = num;
                        }
                        Err(num) => {
                            println!("Err Signal Strength: {}", num);
                            return None;
                        }
                    }
                }
                _ => {
                    println!("Extra data? {}, {}", i, string)
                }
            }
            i += 1;
        } else {
            return None;
        }
    }

    if i >= 5 {
        Some(aquabot_data)
    } else {
        None
    }
}
