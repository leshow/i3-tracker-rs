use crate::{
    error::TrackErr,
    i3log::{Event, I3Log},
};

use futures::{stream::Stream, sync::mpsc::Sender, Future, Sink};
use log::{error, info};
use tokio::{codec::FramedRead, runtime::current_thread::Handle};
use tokio_i3ipc::{
    codec::EventCodec,
    event::{self, Subscribe, WindowChange},
    subscribe_future, Connect, I3,
};

pub fn listen_loop(tx: Sender<Event>, rt: Handle) -> Result<(), TrackErr> {
    let mut prev_new_id = None;

    let fut = I3::connect()?
        .and_then(|stream| subscribe_future(stream, &[Subscribe::Window]))
        .and_then(move |(stream, _)| {
            let frame = FramedRead::new(stream, EventCodec);
            let sender = frame
                .for_each(move |evt: event::Event| {
                    info!("Received: {:#?}", &evt);
                    let tx = tx.clone();
                    if let event::Event::Window(e) = evt {
                        info!("Window event type: {:#?}", &e.change);
                        let id = e.container.id;
                        match e.change {
                            WindowChange::New => {
                                prev_new_id = Some(id);
                            }
                            WindowChange::Focus => {
                                if let Some(prev_id) = prev_new_id {
                                    if prev_id == id {
                                        prev_new_id = None;
                                    }
                                }
                            }
                            _ => {}
                        };
                        prev_new_id = None;
                        match e.change {
                            WindowChange::Focus | WindowChange::Title => {
                                let log = I3Log::from_i3(id, &e.container);
                                info!("Window change, send log event: {:#?}", log);
                                tokio::spawn(tx.send(Event::I3(log)).map(|_| ()).map_err(|_| ()));
                            }
                            _ => {}
                        };
                    }
                    futures::future::ok(())
                })
                .map_err(|e| error!("{:?}", e));
            tokio::spawn(sender);
            Ok(())
        })
        .map(|_| ())
        .map_err(|e| error!("{:?}", e));
    rt.spawn(fut).expect("Failed to run listen future");
    Ok(())
}