extern crate chrono;
extern crate csv;
extern crate i3ipc;
#[macro_use]
extern crate serde_derive;
extern crate futures;
extern crate serde;
extern crate tokio_core;
extern crate xcb;

mod error;
mod time_tracker;

use csv::{Reader, Writer, WriterBuilder};
use error::TrackErr;
use futures::prelude::*;
use futures::sync::mpsc::{self, Receiver, Sender};
use i3ipc::I3EventListener;
use i3ipc::Subscription;
use i3ipc::event::Event;
use i3ipc::event::inner::WindowChange;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::thread;
use std::time::Duration;
use time_tracker::{Log, LogEvent};
use tokio_core::reactor::{Core, Interval};

fn main() {
    if let Err(e) = run("output.log") {
        panic!("{:?}", e);
    };
}

fn run<P: AsRef<Path>>(out_path: P) -> Result<(), TrackErr> {
    let mut writer = csv_writer(&out_path)?;
    let mut i3_listener = I3EventListener::connect()?;

    let (xorg_conn, _) = xcb::Connection::connect(None)?;
    i3_listener.subscribe(&[Subscription::Window])?;

    let mut core = Core::new()?;
    let handle = core.handle();

    // log interval
    let (tx, rx) = mpsc::channel(1024);
    let tx_ = tx.clone();
    let interval = Interval::new(Duration::from_secs(1), &handle)?
        .for_each(move |_| {
            println!("foo");
            if let Event::WindowEvent(e) = i3_listener.listen().last().unwrap().unwrap() {
                let tx_ = tx_.clone();
                let window_id = e.container.window.unwrap_or(-1) as u32;
                let window_class = LogEvent::get_class(&xorg_conn, window_id);
                let window_title = e.container
                    .name
                    .clone()
                    .unwrap_or_else(|| "Untitled".into());

                tx_.send(LogEvent::new(window_id, window_class, window_title))
                    .wait()
                    .unwrap();
            }
            Ok(())
        })
        .map_err(|_| ());

    // spawn main loop
    let h2 = thread::spawn(move || {
        let tx = tx.clone();
        listen_loop(tx).unwrap();
    });

    // receive and write
    let mut next_id = initial_event_id(&out_path)?;
    let f2 = rx.for_each(|log| {
        println!("{:?}", log);
        Log::new(next_id, log).write(&mut writer).unwrap();
        next_id += 1;
        Ok(())
    });

    handle.spawn(interval);
    core.run(f2).expect("Core failed");
    h2.join().unwrap();
    Ok(())
}

fn listen_loop(tx: Sender<LogEvent>) -> Result<(), TrackErr> {
    let mut i3_listener = I3EventListener::connect()?;
    let (xorg_conn, _) = xcb::Connection::connect(None)?;

    let subs = [Subscription::Window];
    i3_listener.subscribe(&subs)?;
    let mut prev_new_window_id: Option<i32> = None;
    for event in i3_listener.listen() {
        println!("bar");
        let tx = tx.clone();
        if let Event::WindowEvent(e) = event? {
            let window_id = e.container.window.unwrap_or(-1);
            if window_id < 1 {
                continue;
            }
            // new window events get duplicated in the listen loop so we need
            // to track a "new" event and ensure we only actually emit the title
            // event
            match e.change {
                WindowChange::New => {
                    prev_new_window_id = Some(window_id);
                    continue;
                }
                WindowChange::Focus => {
                    if let Some(prev_window_id) = prev_new_window_id {
                        if prev_window_id == window_id {
                            prev_new_window_id = None;
                            continue;
                        }
                    }
                }
                _ => {}
            };
            prev_new_window_id = None;
            match e.change {
                WindowChange::Focus | WindowChange::Title => {
                    let window_class = LogEvent::get_class(&xorg_conn, window_id as u32);
                    let window_title = e.container
                        .name
                        .clone()
                        .unwrap_or_else(|| "Untitled".into());
                    let send_event = LogEvent::new(window_id as u32, window_class, window_title);
                    tx.send(send_event).wait().unwrap();
                }
                _ => {}
            };
        }
    }
    Ok(())
}

fn initial_event_id<P: AsRef<Path>>(path: P) -> Result<u32, TrackErr> {
    if let Ok(f) = OpenOptions::new().read(true).open(path) {
        let mut r = Reader::from_reader(f);
        if let Some(res) = r.deserialize().last() {
            let log: Log = res?;
            return Ok(log.id + 1);
        }
    }
    Ok(1)
}

fn csv_writer<P: AsRef<Path>>(path: P) -> Result<Writer<File>, TrackErr> {
    let has_headers = !Path::new(path.as_ref()).exists();
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.as_ref())?;
    let wtr = WriterBuilder::new()
        .has_headers(has_headers)
        .from_writer(file);
    Ok(wtr)
}

// every 60 seconds if no log has been written then write it,
// otherwise keep writing
