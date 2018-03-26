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
use futures::sync::mpsc;
use i3ipc::I3EventListener;
use i3ipc::Subscription;
use i3ipc::event::Event;
use i3ipc::event::inner::WindowChange;
use std::borrow::ToOwned;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    let mut next_id = next_event_id(&out_path)?;
    let mut writer = csv_writer(&out_path)?;
    let mut i3_listener = I3EventListener::connect()?;

    let (xorg_conn, _) = xcb::Connection::connect(None)?;
    i3_listener.subscribe(&[Subscription::Window])?;

    let mut core = Core::new()?;
    let handle = core.handle();

    let (tx, rx) = mpsc::channel(1024);
    let own_path = Arc::new(out_path.as_ref().to_owned());
    let mut i3_listener2 = I3EventListener::connect()?;
    i3_listener2.subscribe(&[Subscription::Window]);
    let xorg = Arc::new(xorg_conn.clone());

    let tx_ = tx.clone();
    let interval = Interval::new(Duration::from_secs(60), &handle)?
        .for_each(move |_| {
            let tx = tx_.clone();
            if let Event::WindowEvent(e) = i3_listener2.listen().last().unwrap().unwrap() {
                let mut next_id = next_event_id(own_path.as_ref()).unwrap();
                tx.send(LogEvent::new(next_id, &e, &xorg));
            }
            futures::future::ok(())
        })
        .map_err(|_| ());
    handle.spawn(interval);

    // thread::spawn(move || {
    let mut current_event: Option<LogEvent> = None;
    let mut last_event_new: bool = false;
    for event in i3_listener.listen() {
        let tx_ = tx.clone();
        if let Event::WindowEvent(e) = event? {
            match e.change {
                WindowChange::New => {
                    last_event_new = true;
                }
                WindowChange::Focus => {
                    if last_event_new {
                        last_event_new = false;
                        continue;
                    }
                    if let Some(e) = current_event {
                        // Log::new(e).write(&mut writer)?;
                        tx_.send(e);
                    }
                    current_event = Some(LogEvent::new(next_id, &e, &xorg_conn));
                    next_id += 1;
                }
                WindowChange::Title => {
                    last_event_new = false;
                    if let Some(e) = current_event {
                        // Log::new(e).write(&mut writer)?;
                        tx_.send(e);
                    }
                    current_event = Some(LogEvent::new(next_id, &e, &xorg_conn));
                    next_id += 1
                }
                _ => {}
            };
        }
    }

    //     Ok(())
    // });

    let f2 = rx.for_each(|log_event| {
        Log::new(log_event).write(&mut writer).unwrap();
        Ok(())
    });
    core.run(f2).expect("Core failed");
    Ok(())
}

fn next_event_id<P: AsRef<Path>>(path: P) -> Result<u32, TrackErr> {
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
