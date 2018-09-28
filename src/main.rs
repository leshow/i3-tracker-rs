#![feature(await_macro, async_await, futures_api)]

#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate tokio;

mod error;
mod log;
mod win;

pub(crate) use crate::{
    error::TrackErr,
    log::{Event, I3Log, Log},
};

use {
    futures::prelude::*,
    futures::sync::mpsc::{self, Sender},
    std::{
        io, thread,
        time::{Duration, Instant},
    },
    tokio::runtime::current_thread::Handle,
    tokio::timer::Delay,
};

const TIMEOUT_DELAY: u64 = 10;
const LOG_LIMIT: usize = 10;
const LOG_BASE_NAME: &str = "i3tracker";

fn main() -> Result<(), TrackErr> {
    let log_path = setup_log()?;
    // log interval
    let (tx, rx) = mpsc::channel(50);
    let mut next_id = log::initial_event_id(&log_path);

    // catch exit & write to log
    let mut rt = tokio::runtime::current_thread::Runtime::new().expect("Failed building runtime");
    rt.spawn(sigint(tx.clone()));

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
    let handle: Handle = rt.handle();
    let f2 = rx.for_each(move |event| {
        match event {
            Event::I3(e) => {
                if let Some(ref prev) = prev_i3log {
                    Log::new(next_id, prev)
                        .write(&mut writer)
                        .expect("write failed");
                    next_id += 1;
                }

                handle.spawn(timeout(tx.clone(), next_id));
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
                handle.spawn(timeout(tx.clone(), next_id));
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
    rt.spawn(f2);
    rt.run().expect("Failed runtime");
    Ok(())
}

fn timeout(tx: Sender<Event>, id: u32) -> impl Future<Item = (), Error = ()> {
    Delay::new(Instant::now() + Duration::from_secs(TIMEOUT_DELAY))
        .map(|_| ())
        .map_err(|_| ())
        .and_then(move |_| tx.send(Event::Tick(id)).map_err(|_| ()))
        .map(|_| ())
        .map_err(|_| ())
}

fn sigint(tx: Sender<Event>) -> impl Future<Item = (), Error = ()> {
    tokio_signal::ctrl_c()
        .flatten_stream()
        .for_each(move |_| {
            tx.clone()
                .send(Event::Flush)
                .map(|_| ())
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        })
        .map_err(|_| ())
}

use std::{fs, path::Path};

fn setup_log() -> Result<impl AsRef<Path>, TrackErr> {
    // get data dir
    let xdg_dir = xdg::BaseDirectories::with_prefix(LOG_BASE_NAME)?;
    let cur_log = rotate(xdg_dir.get_data_home().as_path(), LOG_LIMIT)?;

    Ok(xdg_dir.place_data_file(format!("{}{}.{}", LOG_BASE_NAME, ".log", cur_log))?)
}

fn rotate<P: AsRef<Path>>(dir: P, num: usize) -> Result<usize, TrackErr> {
    let mut files = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .file_stem()
            .map(|h| {
                h.to_str()
                    .map(|g| g.starts_with(LOG_BASE_NAME))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
        {
            let modif = path.metadata()?.modified()?.elapsed()?.as_secs();
            files.push((path, modif));
        }
    }

    if files.len() >= num {
        files.sort_by(|a, b| (a.1).cmp(&b.1));

        if let Some((last, _)) = files.first() {
            if let Some(Some(Ok(n))) = last
                .extension()
                .map(|c| c.to_str().map(|c| c.to_owned().parse::<usize>()))
            {
                return Ok(n + 1);
            }
        }
        Ok(0)
    } else {
        Ok(files.len())
    }
}
