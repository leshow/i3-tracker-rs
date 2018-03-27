#![feature(conservative_impl_trait)]
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

use csv::{Reader, Writer, WriterBuilder};
use error::TrackErr;
use futures::prelude::*;
use futures::sync::mpsc::{self, Sender};
use i3ipc::I3EventListener;
use i3ipc::Subscription;
use i3ipc::event::Event;
use i3ipc::event::inner::WindowChange;
use log::{I3Event, Log, LogEvent};
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::thread;
use std::time::Duration;
use tokio_core::reactor::{Core, Handle, Timeout};

const DELAY: u64 = 2;

fn main() {
    if let Err(e) = run("output.log") {
        panic!("{:?}", e);
    };
}

fn run<P: AsRef<Path>>(out_path: P) -> Result<(), TrackErr> {
    // make core
    let mut core = Core::new()?;
    let handle = core.handle();
    // log interval
    let (tx, rx) = mpsc::channel(100);
    // spawn listen loop
    {
        let tx = tx.clone();
        thread::spawn(move || {
            listen_loop(tx).unwrap();
        });
    }
    let mut writer = csv_writer(&out_path)?;
    // receive and write
    let mut next_id = initial_event_id(&out_path)?;
    let mut last_log: Option<LogEvent> = None;

    let f2 = rx.for_each(move |event| {
        match event {
            I3Event::Log(log) => {
                println!("{} - {:?}", next_id, log);
                Log::new(next_id, &log).write(&mut writer).unwrap();
                next_id += 1;
                handle.spawn(timeout(tx.clone(), &handle, next_id));

                last_log = Some(log);
            }
            I3Event::Last(id) => {
                if next_id != id {
                    println!("do last: {} - {:?}", id, last_log);
                    if let Some(ref log) = last_log {
                        Log::new(next_id, log).write(&mut writer).unwrap();
                        next_id += 1;
                    }
                    handle.spawn(timeout(tx.clone(), &handle, id));
                }
            }
        }
        Ok(())
    });

    core.run(f2).expect("Core failed");
    Ok(())
}

fn timeout(tx: Sender<I3Event>, handle: &Handle, id: u32) -> impl Future<Item = (), Error = ()> {
    Timeout::new(Duration::from_secs(DELAY), &handle)
        .unwrap()
        .then(move |_| {
            tx.send(I3Event::Last(id)).wait().unwrap();
            Ok(())
        })
}

fn listen_loop(tx: Sender<I3Event>) -> Result<(), TrackErr> {
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
                    let event = LogEvent::new(window_id as u32, &xorg_conn, &e);
                    tx.send(I3Event::Log(event)).wait().unwrap();
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
