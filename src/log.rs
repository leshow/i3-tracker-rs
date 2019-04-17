use crate::{error::TrackErr, i3};

use chrono::{DateTime, Local};
use csv::{Reader, Writer, WriterBuilder};
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    io::{self, ErrorKind},
    path::Path,
};
use tokio_i3ipc::event::WindowData;
use xcb;

pub enum Event {
    I3(I3Log),
    Tick(u32),
    Flush,
}

#[derive(Debug, Clone)]
pub struct I3Log {
    pub start_time: DateTime<Local>,
    pub window_id: u32,
    pub window_class: String,
    pub window_title: String,
}

impl I3Log {
    pub fn from_i3(window_id: u32, xorg_conn: &xcb::Connection, e: &WindowData) -> I3Log {
        I3Log {
            start_time: Local::now(),
            window_id,
            window_class: i3::get_class(xorg_conn, window_id as u32),
            window_title: e
                .container
                .name
                .clone()
                .unwrap_or_else(|| "Untitled".into()),
        }
    }

    pub fn new_start(&self) -> Self {
        I3Log {
            start_time: Local::now(),
            window_id: self.window_id,
            window_class: self.window_class.clone(),
            window_title: self.window_title.clone(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Log {
    pub id: u32,
    pub start_time: String,
    pub end_time: String,
    pub duration: i64,
    pub window_id: u32,
    pub window_class: String,
    pub window_title: String,
}

impl Log {
    pub fn new(id: u32, e: &I3Log) -> Log {
        let now = Local::now();
        let elapsed = now.signed_duration_since(e.start_time);
        Log {
            id,
            window_id: e.window_id,
            window_class: e.window_class.clone(),
            window_title: e.window_title.clone(),
            duration: elapsed.num_seconds(),
            start_time: e.start_time.format("%F %T").to_string(),
            end_time: now.format("%F %T").to_string(),
        }
    }

    pub fn write(&self, writer: &mut Writer<File>) -> Result<(), TrackErr> {
        writer.serialize(self)?;
        writer.flush()?;
        Ok(())
    }

    pub fn read<P: AsRef<Path>>(path: P) -> Result<Log, TrackErr> {
        if let Ok(f) = OpenOptions::new().read(true).open(path) {
            let mut r = Reader::from_reader(f);
            if let Some(res) = r.deserialize().last() {
                let log: Log = res?;
                return Ok(log);
            }
        }
        Err(TrackErr::Io(io::Error::new(
            ErrorKind::NotFound,
            "output not found",
        )))
    }
}

pub fn initial_event_id<P: AsRef<Path>>(path: P) -> u32 {
    match Log::read(path) {
        Ok(Log { id, .. }) => id + 1,
        Err(_) => 1,
    }
}

pub fn writer<P: AsRef<Path>>(path: P) -> Result<Writer<File>, TrackErr> {
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
