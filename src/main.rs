#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate log;

mod error;
mod i3;
mod i3log;

pub(crate) use crate::{
    error::TrackErr,
    i3log::{Event, I3Log, Log},
};
use futures::{
    channel::mpsc::{self, Sender},
    prelude::*,
};
use std::{io, path::Path};
use tokio::{
    fs,
    signal::ctrl_c,
    time::{self, Duration, Instant},
};

const TIMEOUT_DELAY: u64 = 10;
const LOG_LIMIT: usize = 10;
const LOG_BASE_NAME: &str = "i3tracker";

#[tokio::main(basic_scheduler)]
async fn main() -> Result<(), TrackErr> {
    env_logger::init();
    let log_path = setup_log().await?;
    // log interval
    info!("Creating listen channel");
    let (tx, mut rx) = mpsc::channel(50);
    // catch exit & write to log
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            sigint(tx).await.expect("Failed to get SIGINT");
        });
    }

    // spawn listen loop
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            i3::listen_loop(tx).await.expect("Listen loop crashed");
        });
    }

    let mut next_id = i3log::initial_event_id(&log_path);
    info!("Next id from logs is {:?}", next_id);

    let mut writer = i3log::writer(&log_path)?;
    let mut prev_i3log: Option<I3Log> = None;
    // consume events

    while let Some(event) = rx.next().await {
        match event {
            Event::I3(e) => {
                if let Some(ref prev) = prev_i3log {
                    Log::new(next_id, prev)
                        .write(&mut writer)
                        .expect("write failed");
                    next_id += 1;
                }
                let tx = tx.clone();
                tokio::spawn(async move {
                    timeout(tx, next_id).await.expect("Timeout failed");
                });
                prev_i3log = Some(e);
            }
            Event::Tick(id) => {
                if next_id != id {
                    continue;
                }
                if let Some(ref prev) = prev_i3log {
                    info!("Tick - writing log");
                    Log::new(next_id, prev)
                        .write(&mut writer)
                        .expect("write failed!");
                    prev_i3log = Some(prev.new_start());
                }
                let tx = tx.clone();
                tokio::spawn(async move {
                    timeout(tx, id).await.expect("Timeout failed");
                });
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
    }
    Ok(())
}

async fn timeout(mut tx: Sender<Event>, id: u32) -> io::Result<()> {
    time::delay_until(Instant::now() + Duration::from_secs(TIMEOUT_DELAY)).await;
    tx.send(Event::Tick(id))
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()))?;
    Ok(())
}

async fn sigint(tx: Sender<Event>) -> io::Result<()> {
    ctrl_c().await?;
    tx.clone()
        .send(Event::Flush)
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::Interrupted, e.to_string()))?;
    Ok(())
}

// TODO: This function has a mix of async and sync fs operations that's not
// great
async fn setup_log() -> Result<impl AsRef<Path>, TrackErr> {
    // get data dir
    let xdg_dir = xdg::BaseDirectories::with_prefix(LOG_BASE_NAME)?;
    let home = xdg_dir.get_data_home();
    info!("Setting up log in {:?}", home.as_path());
    let cur_log = rotate(home.as_path(), LOG_LIMIT).await?;
    info!("Current log is {:?}", cur_log);

    Ok(xdg_dir.place_data_file(format!("{}{}.{}", LOG_BASE_NAME, ".log", cur_log))?)
}

async fn rotate<P: AsRef<Path>>(dir: P, num: usize) -> Result<usize, TrackErr> {
    let mut files = Vec::new();
    let mut readdir = fs::read_dir(dir.as_ref()).await?;

    while let Some(entry) = readdir.next().await {
        let entry = entry?;
        let path = entry.path();
        let found_log = path
            .file_stem()
            .map(|h| {
                h.to_str()
                    .map(|g| g.starts_with(LOG_BASE_NAME))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if found_log {
            let modif = path.metadata()?.modified()?.elapsed()?.as_secs();
            files.push((path, modif));
        }
    }

    if files.len() >= num {
        files.sort_by(|&(_, a), &(_, ref b)| a.cmp(b));

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
