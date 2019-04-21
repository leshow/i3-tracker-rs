use crate::{
    error::TrackErr,
    log::{Event, I3Log},
};

use futures::{stream::Stream, sync::mpsc::Sender, Future, Sink};
use tokio::{codec::FramedRead, runtime::current_thread::Handle};
use tokio_i3ipc::{
    codec::EventCodec,
    event::{self, Subscribe, WindowChange},
    subscribe_future, Connect, I3,
};
use xcb;

pub fn listen_loop(tx: Sender<Event>, rt: Handle) -> Result<(), TrackErr> {
    let (xorg_conn, _) = xcb::Connection::connect(None)?;
    let mut prev_new_window_id = None;

    let fut = I3::connect()?
        .and_then(|stream| subscribe_future(stream, &[Subscribe::Window]))
        .and_then(move |(stream, _)| {
            let frame = FramedRead::new(stream, EventCodec);
            let sender = frame
                .for_each(move |evt: event::Event| {
                    let tx = tx.clone();
                    if let event::Event::Window(e) = evt {
                        let window_id = e.container.id;
                        match e.change {
                            WindowChange::New => {
                                prev_new_window_id = Some(window_id);
                            }
                            WindowChange::Focus => {
                                if let Some(prev_window_id) = prev_new_window_id {
                                    if prev_window_id == window_id {
                                        prev_new_window_id = None;
                                    }
                                }
                            }
                            _ => {}
                        };
                        prev_new_window_id = None;
                        match e.change {
                            WindowChange::Focus | WindowChange::Title => {
                                let log = I3Log::from_i3(window_id as u32, &xorg_conn, &e);
                                tokio::spawn(tx.send(Event::I3(log)).map(|_| ()).map_err(|_| ()));
                            }
                            _ => {}
                        };
                    }
                    futures::future::ok(())
                })
                .map_err(|e| eprintln!("{:?}", e));
            tokio::spawn(sender);
            Ok(())
        })
        .map(|_| ())
        .map_err(|e| eprintln!("{:?}", e));
    rt.spawn(fut);
    Ok(())
}

// pulled from:
// https://stackoverflow.com/questions/44833160/how-do-i-get-the-x-window-class-given-a-window-id-with-rust-xcb
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
