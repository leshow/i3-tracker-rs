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
extern crate xdg;

mod error;
mod log;
mod win;

pub(crate) use error::TrackErr;
pub(crate) use log::{Event, I3Log, Log};

use futures::prelude::*;
use futures::sync::mpsc::{self, Sender};
use std::{io, thread, time::Duration};
use tokio_core::reactor::{Core, Handle, Timeout};

const TIMEOUT_DELAY: u64 = 10;
const LOG_BASE_NAME: &'static str = "i3tracker.log";

fn main() {
    if let Err(e) = run() {
        panic!("{}", e);
    };
}

fn run() -> Result<(), TrackErr> {
    // get data dir
    let xdg_dirs = xdg::BaseDirectories::with_prefix("i3tracker")?;
    let log_path = xdg_dirs.place_data_file(LOG_BASE_NAME)?;

    let mut core = Core::new()?;
    let handle = core.handle();
    // log interval
    let (tx, rx) = mpsc::channel(100);
    let mut next_id = log::initial_event_id(&log_path);

    // catch exit & write to log
    handle.spawn(sigint(tx.clone(), &handle));

    // spawn listen loop
    {
        let tx = tx.clone();
        thread::spawn(move || {
            win::listen_loop(tx).unwrap();
        });
    }

    let mut writer = log::writer(&log_path)?;
    let mut prev_i3log: Option<I3Log> = None;
    // consume events
    let f2 = rx.for_each(move |event| {
        match event {
            Event::I3(e) => {
                if let Some(ref prev) = prev_i3log {
                    Log::new(next_id, prev)
                        .write(&mut writer)
                        .expect("write failed");
                    next_id += 1;
                }
                handle.spawn(timeout(tx.clone(), &handle, next_id));
                prev_i3log = Some(e);
            }
            Event::Tick(id) => {
                if next_id != id {
                    return Ok(());
                }
                // dirty borrowck hack
                let prev_outer = match prev_i3log {
                    Some(ref prev) => {
                        Log::new(next_id, &prev)
                            .write(&mut writer)
                            .expect("write failed");
                        next_id += 1;
                        Some(prev.new_start()) // b/c prev_i3log is borrowed in here we can't re-assign
                    }
                    None => None,
                };
                if let Some(prev) = prev_outer {
                    prev_i3log = Some(prev);
                }
                handle.spawn(timeout(tx.clone(), &handle, next_id));
            }
            Event::Flush => {
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
    Timeout::new(Duration::from_secs(TIMEOUT_DELAY), &handle)
        .expect("Timeout failed")
        .and_then(move |_| {
            tx.send(Event::Tick(id))
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        })
        .map(|_| ())
        .map_err(|_| ())
}

fn sigint(tx: Sender<Event>, h: &Handle) -> impl Future<Item = (), Error = ()> {
    tokio_signal::ctrl_c(&h)
        .flatten_stream()
        .for_each(move |_| {
            let tx = tx.clone();
            tx.send(Event::Flush)
                .map(|_| ())
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        })
        .map_err(|_| ())
}
