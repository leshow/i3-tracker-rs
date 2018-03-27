use chrono::prelude::*;
use csv::Writer;
use error::TrackErr;
use i3ipc::event::WindowEventInfo;
use std::fs::File;
use xcb;

pub enum I3Event {
    Log(LogEvent),
    Last(),
}

#[derive(Debug, Clone)]
pub struct LogEvent {
    start_time: DateTime<Local>,
    window_id: u32,
    window_class: String,
    window_title: String,
}

impl LogEvent {
    // pub fn new(window_id: u32, window_class: String, window_title: String) ->
    // LogEvent {     LogEvent {
    //         start_time: Local::now(),
    //         window_id,
    //         window_class,
    //         window_title,
    //     }
    // }
    pub fn new(window_id: u32, xorg_conn: &xcb::Connection, e: &WindowEventInfo) -> LogEvent {
        LogEvent {
            start_time: Local::now(),
            window_id,
            window_class: LogEvent::get_class(&xorg_conn, window_id as u32),
            window_title: e.container
                .name
                .clone()
                .unwrap_or_else(|| "Untitled".into()),
        }
    }
    /*
     * pulled from:
     * https://stackoverflow.com/questions/44833160/how-do-i-get-the-x-window-class-given-a-window-id-with-rust-xcb
     */
    pub fn get_class(conn: &xcb::Connection, window_id: u32) -> String {
        let window: xcb::xproto::Window = window_id;
        let long_length: u32 = 8;
        let mut long_offset: u32 = 0;
        let mut buf = Vec::new();
        loop {
            let cookie = xcb::xproto::get_property(
                conn,
                false,
                window,
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
    start_time: String,
    end_time: String,
    duration: i64,
    window_id: u32,
    window_class: String,
    window_title: String,
}

impl Log {
    pub fn new(id: u32, e: &LogEvent) -> Log {
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
}
