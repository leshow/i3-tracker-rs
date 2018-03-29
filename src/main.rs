#![feature(nll)]
extern crate chrono;
extern crate csv;
extern crate i3ipc;
#[macro_use]
extern crate serde_derive;
extern crate fs2;
extern crate futures;
extern crate serde;
extern crate tokio_core;
extern crate tokio_signal;
extern crate xcb;

mod error;
mod log;

pub(crate) use error::TrackErr;
pub(crate) use log::{Event, I3Log, Log};

use csv::{Writer, WriterBuilder};
use fs2::FileExt;
use futures::prelude::*;
use futures::sync::mpsc::{self, Sender};
use i3ipc::{I3EventListener, Subscription, event::{Event as WinEvent, inner::WindowChange}};
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

    // catch exit
    handle.spawn(sigint(tx.clone(), &handle));

    // spawn listen loop
    {
        let tx = tx.clone();
        thread::spawn(move || {
            listen_loop(tx).unwrap();
        });
    }

    let mut writer = csv_writer(&out_path)?;
    let mut prev_i3log: Option<I3Log> = None;
    // consume events
    let f2 = rx.for_each(move |event| {
        match event {
            | Event::I3(e) => {
                if let Some(ref prev) = prev_i3log {
                    Log::new(next_id, prev)
                        .write(&mut writer)
                        .expect("write failed");
                    next_id += 1;
                }
                handle.spawn(timeout(tx.clone(), &handle, next_id));
                prev_i3log = Some(e);
            }
            | Event::Tick(id) => {
                if next_id != id {
                    return Ok(());
                }
                if let Some(ref prev) = prev_i3log {
                    Log::new(next_id, prev)
                        .write(&mut writer)
                        .expect("write failed");
                    next_id += 1;
                    prev_i3log = Some(prev.new_start());
                }
                handle.spawn(timeout(tx.clone(), &handle, next_id));
            }
            | Event::Flush => {
                if let Some(ref prev) = prev_i3log {
                    Log::new(next_id, prev)
                        .write(&mut writer)
                        .expect("write failed");
                }
                std::process::exit(0);
            }
        }
        Ok(())
    });
    core.run(f2).expect("Core failed");
    Ok(())
}

fn timeout(tx: Sender<Event>, handle: &Handle, id: u32) -> impl Future<Item = (), Error = ()> {
    Timeout::new(Duration::from_secs(DELAY), &handle)
        .expect("Timeout failed")
        .then(move |_| {
            tx.send(Event::Tick(id)).wait().unwrap();
            Ok(())
        })
}

fn listen_loop(tx: Sender<Event>) -> Result<(), TrackErr> {
    let mut i3_listener = I3EventListener::connect()?;
    let (xorg_conn, _) = xcb::Connection::connect(None)?;
    let subs = [Subscription::Window];
    i3_listener.subscribe(&subs)?;
    let mut prev_new_window_id = None;

    for event in i3_listener.listen() {
        let tx = tx.clone();
        if let WinEvent::WindowEvent(e) = event? {
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
                    let log = I3Log::new(window_id as u32, &xorg_conn, &e);
                    tx.send(Event::I3(log)).wait().unwrap();
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
    file.try_lock_exclusive()?;
    let wtr = WriterBuilder::new()
        .has_headers(has_headers)
        .from_writer(file);
    Ok(wtr)
}

fn sigint(tx: Sender<Event>, h: &Handle) -> impl Future<Item = (), Error = ()> {
    tokio_signal::ctrl_c(&h)
        .flatten_stream()
        .for_each(move |()| {
            let tx = tx.clone();
            tx.send(Event::Flush).wait().unwrap();
            Ok(())
        })
        .map_err(|_| ())
}
