use crate::{
    error::TrackErr,
    i3log::{Event, I3Log},
};

use futures::{channel::mpsc::Sender, sink::SinkExt, stream::StreamExt};
use log::info;
use tokio_i3ipc::{
    event::{Event as I3Event, Subscribe, WindowChange},
    I3,
};

pub async fn listen_loop(tx: Sender<Event>) -> Result<(), TrackErr> {
    let mut prev_new_id = None;

    let mut i3 = I3::connect().await?;
    let _resp = i3.subscribe([Subscribe::Window]).await?;

    let mut listener = i3.listen();
    while let Some(evt) = listener.next().await {
        info!("Received: {:#?}", &evt);
        if let I3Event::Window(e) = evt? {
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
            match e.change {
                WindowChange::Focus | WindowChange::Title => {
                    let log = I3Log::from_i3(id, &e.container);
                    info!("Window change, send log event: {:#?}", log);
                    let mut tx = tx.clone();
                    tokio::spawn(async move {
                        tx.send(Event::I3(log)).await.expect("Send i3 log failed");
                    });
                }
                _ => {}
            };
        }
    }
    Ok(())
}
