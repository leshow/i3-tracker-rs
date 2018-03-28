#![feature(nll)]
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
mod log;

pub use error::TrackErr;
pub(crate) use log::{I3LogEvent, Log, LogEvent};

use csv::{Writer, WriterBuilder};
use futures::prelude::*;
use futures::sync::mpsc::{self, Sender};
use i3ipc::event::{Event, inner::WindowChange};
use i3ipc::{I3EventListener, Subscription};
use std::fs::{File, OpenOptions};
use std::{thread, path::Path, time::Duration};
use tokio_core::reactor::{Core, Handle, Timeout};

const DELAY: u64 = 10;

fn main() {
    if let Err(e) = run("output.log") {
        panic!("{:?}", e);
    };
}

fn run<P: AsRef<Path>>(out_path: P) -> Result<(), TrackErr> {
    let mut core = Core::new()?;
    let handle = core.handle();
    // log interval
    let (tx, rx) = mpsc::channel(100);
    let mut next_id = log::initial_event_id(&out_path);

    // spawn listen loop
    {
        let tx = tx.clone();
        thread::spawn(move || {
            listen_loop(tx).unwrap();
        });
    }
    let mut writer = csv_writer(&out_path)?;
    let mut prev_i3_event: Option<I3LogEvent> = None;
    // consume events
    let f2 = rx.for_each(move |event| {
        match event {
            LogEvent::I3Event(e) => {
                if let Some(ref prev) = prev_i3_event {
                    Log::new(next_id, prev)
                        .write(&mut writer)
                        .expect("write failed");
                    next_id += 1;
                }
                handle.spawn(timeout(tx.clone(), &handle, next_id));
                prev_i3_event = Some(e);
            }
            LogEvent::Tick(id) => {
                if next_id != id {
                    return Ok(());
                }
                if let Some(ref prev) = prev_i3_event {
                    Log::new(next_id, prev)
                        .write(&mut writer)
                        .expect("write failed");
                    next_id += 1;
                    prev_i3_event = Some(prev.new_time());
                }
                handle.spawn(timeout(tx.clone(), &handle, next_id));
            }
        }
        Ok(())
    });
    core.run(f2).expect("Core failed");
    Ok(())
}

fn timeout(tx: Sender<LogEvent>, handle: &Handle, id: u32) -> impl Future<Item = (), Error = ()> {
    Timeout::new(Duration::from_secs(DELAY), &handle)
        .unwrap()
        .then(move |_| {
            tx.send(LogEvent::Tick(id)).wait().unwrap();
            Ok(())
        })
}

fn listen_loop(tx: Sender<LogEvent>) -> Result<(), TrackErr> {
    let mut i3_listener = I3EventListener::connect()?;
    let (xorg_conn, _) = xcb::Connection::connect(None)?;
    let subs = [Subscription::Window];
    i3_listener.subscribe(&subs)?;
    let mut prev_new_window_id: Option<i32> = None;

    for event in i3_listener.listen() {
        let tx = tx.clone();
        if let Event::WindowEvent(e) = event? {
            let window_id = e.container.window.unwrap_or(-1);
            if window_id < 1 {
                continue;
            }
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
                    let event = I3LogEvent::new(window_id as u32, &xorg_conn, &e);
                    tx.send(LogEvent::I3Event(event)).wait().unwrap();
                }
                _ => {}
            };
        }
    }
    Ok(())
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

// fn interval(tx: Sender<LogEvent>, handle: &Handle) -> impl Future<Item = (),
// Error = ()> {     Interval::new(Duration::from_secs(30), &handle)
//         .unwrap()
//         .for_each(move |_| {
//             let tx_ = tx.clone();
//             tx_.send(LogEvent::Interval).wait().unwrap();
//             Ok(())
//         })
//         .map_err(|_| ())
// }
