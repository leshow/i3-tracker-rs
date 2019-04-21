use crate::{error::TrackErr, i3};

use chrono::{DateTime, Local};
use csv::{Reader, Writer, WriterBuilder};
use fs2::FileExt;
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{self, ErrorKind},
    path::Path,
};
use tokio_i3ipc::{
    event::WindowData,
    reply::{NodeType, WindowProperty},
};
use xcb;

#[derive(Debug, Eq, PartialEq)]
pub enum Event {
    I3(I3Log),
    Tick(u32),
    Flush,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct I3Log {
    pub start_time: DateTime<Local>,
    pub window_id: u32,
    pub window_class: String,
    pub window_title: Option<String>,
    pub node_type: NodeType,
    pub output: Option<String>,
    pub class: Option<String>,
    pub role: Option<String>,
}

impl I3Log {
    pub fn from_i3(window_id: u32, xorg_conn: &xcb::Connection, e: &WindowData) -> I3Log {
        let get_prop =
            |map: &Option<HashMap<WindowProperty, Option<String>>>, prop| -> Option<String> {
                map.as_ref()
                    .and_then(|map| map.get(&prop).and_then(Clone::clone))
            };
        let class = get_prop(&e.container.window_properties, WindowProperty::Class);
        let role = get_prop(&e.container.window_properties, WindowProperty::WindowRole);

        I3Log {
            start_time: Local::now(),
            window_id,
            window_class: i3::get_class(xorg_conn, window_id as u32),
            window_title: e.container.name.clone(),
            node_type: e.container.node_type.clone(),
            output: e.container.output.clone(),
            class,
            role,
        }
    }

    pub fn new_start(&self) -> Self {
        let log = self.clone();
        I3Log {
            start_time: Local::now(),
            ..log
        }
    }
}

#[derive(Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Log {
    pub id: u32,
    pub start_time: String,
    pub end_time: String,
    pub duration: i64,
    pub window_id: u32,
    pub window_title: Option<String>,
    pub window_class: String,
    pub node_type: NodeType,
    pub output: Option<String>,
    pub class: Option<String>,
    pub role: Option<String>,
}

impl Log {
    pub fn new(id: u32, e: &I3Log) -> Log {
        let now = Local::now();
        let elapsed = now.signed_duration_since(e.start_time);
        Log {
            id,
            duration: elapsed.num_seconds(),
            start_time: e.start_time.format("%F %T").to_string(),
            end_time: now.format("%F %T").to_string(),
            window_id: e.window_id,
            window_title: e.window_title.clone(),
            window_class: e.window_class.clone(),
            node_type: e.node_type.clone(),
            output: e.output.clone(),
            class: e.class.clone(),
            role: e.role.clone(),
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
                info!("Deserialized {:?}", res);
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
