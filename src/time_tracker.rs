use chrono::prelude::*;
use csv::Writer;
use error::TrackErr;
use i3ipc::event::WindowEventInfo;
use std::fs::File;
use xcb;

pub struct LogEvent {
    id: u32,
    start_time: DateTime<Local>,
    window_id: i32,
    window_class: String,
    window_title: String,
}

impl LogEvent {
    pub fn new(id: u32, event: &WindowEventInfo, xorg_conn: &xcb::Connection) -> LogEvent {
        let window_id = event.container.window.unwrap_or(-1);

        LogEvent {
            id,
            start_time: Local::now(),
            window_id,
            window_class: LogEvent::get_class(xorg_conn, window_id),
            window_title: event
                .container
                .name
                .clone()
                .unwrap_or_else(|| "Untitled".into()),
        }
    }
    /*
     * pulled from:
     * https://stackoverflow.com/questions/44833160/how-do-i-get-the-x-window-class-given-a-window-id-with-rust-xcb
     */
    pub fn get_class(conn: &xcb::Connection, id: i32) -> String {
        let window: xcb::xproto::Window = id as u32;
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
    window_id: i32,
    window_class: String,
    window_title: String,
}

impl Log {
    pub fn new(event: LogEvent) -> Log {
        let now = Local::now();
        let elapsed = now.signed_duration_since(event.start_time);
        Log {
            id: event.id,
            window_id: event.window_id,
            window_class: event.window_class,
            window_title: event.window_title,
            duration: elapsed.num_seconds(),
            start_time: event.start_time.format("%F %T").to_string(),
            end_time: now.format("%F %T").to_string(),
        }
    }
    pub fn write(&self, writer: &mut Writer<File>) -> Result<(), TrackErr> {
        writer.serialize(self)?;
        writer.flush()?;
        Ok(())
    }
}
