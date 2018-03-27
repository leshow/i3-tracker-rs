use chrono::{DateTime, Local};
use csv::{Reader, Writer};
use error::TrackErr;
use i3ipc::event::WindowEventInfo;
use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind};
use std::path::Path;
use xcb;

pub enum LogEvent {
    I3Event(I3LogEvent),
    Tick(u32),
    Interval,
}

#[derive(Debug, Clone)]
pub struct I3LogEvent {
    pub start_time: DateTime<Local>,
    pub window_id: u32,
    pub window_class: String,
    pub window_title: String,
}

impl I3LogEvent {
    pub fn new(window_id: u32, xorg_conn: &xcb::Connection, e: &WindowEventInfo) -> I3LogEvent {
        I3LogEvent {
            start_time: Local::now(),
            window_id,
            window_class: I3LogEvent::get_class(&xorg_conn, window_id as u32),
            window_title: e.container
                .name
                .clone()
                .unwrap_or_else(|| "Untitled".into()),
        }
    }
    pub fn update_time(&mut self) {
        self.start_time = Local::now();
    }
    pub fn new_time(&self) -> Self {
        I3LogEvent {
            start_time: Local::now(),
            window_id: self.window_id.clone(),
            window_class: self.window_class.clone(),
            window_title: self.window_title.clone(),
        }
    }
    /*
     * pulled from:
     * https://stackoverflow.com/questions/44833160/how-do-i-get-the-x-window-class-given-a-window-id-with-rust-xcb
     */
    pub fn get_class(conn: &xcb::Connection, window_id: u32) -> String {
        let long_length: u32 = 8;
        let mut long_offset: u32 = 0;
        let mut buf = Vec::new();
        loop {
            let cookie = xcb::xproto::get_property(
                conn,
                false,
                window_id,
                xcb::xproto::ATOM_WM_CLASS,
                xcb::xproto::ATOM_STRING,
                long_offset,
                long_length,
            );
            match cookie.get_reply() {
                Ok(reply) => {
                    let value: &[u8] = reply.value();
                    buf.extend_from_slice(value);
                    match reply.bytes_after() {
                        0 => break,
                        _ => {
                            let len = reply.value_len();
                            long_offset += len / 4;
                        }
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }
        let result = String::from_utf8(buf).unwrap();
        let results: Vec<_> = result.split('\0').collect();
        results[0].to_string()
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
    pub fn new(id: u32, e: &I3LogEvent) -> Log {
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
