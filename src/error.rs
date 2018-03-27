use csv;
use i3ipc;
use std::error::Error;
use std::fmt;
use std::io;
use xcb;

#[derive(Debug)]
pub enum TrackErr {
    Io(io::Error),
    Csv(csv::Error),
    Xcb(xcb::ConnError),
    IpcMsg(i3ipc::MessageError),
    IpcConn(i3ipc::EstablishError),
}

impl From<csv::Error> for TrackErr {
    fn from(e: csv::Error) -> TrackErr {
        TrackErr::Csv(e)
    }
}

impl From<xcb::ConnError> for TrackErr {
    fn from(e: xcb::ConnError) -> TrackErr {
        TrackErr::Xcb(e)
    }
}

impl From<io::Error> for TrackErr {
    fn from(e: io::Error) -> TrackErr {
        TrackErr::Io(e)
    }
}

impl From<i3ipc::MessageError> for TrackErr {
    fn from(e: i3ipc::MessageError) -> TrackErr {
        TrackErr::IpcMsg(e)
    }
}

impl From<i3ipc::EstablishError> for TrackErr {
    fn from(e: i3ipc::EstablishError) -> TrackErr {
        TrackErr::IpcConn(e)
    }
}

impl Error for TrackErr {
    fn description(&self) -> &str {
        match *self {
            TrackErr::Io(ref e) => e.description(),
            TrackErr::Csv(ref e) => e.description(),
            TrackErr::Xcb(ref e) => e.description(),
            TrackErr::IpcMsg(ref e) => e.description(),
            TrackErr::IpcConn(ref e) => e.description(),
        }
    }
    fn cause(&self) -> Option<&Error> {
        match *self {
            TrackErr::Io(ref e) => Some(e),
            TrackErr::Csv(ref e) => Some(e),
            TrackErr::Xcb(ref e) => Some(e),
            TrackErr::IpcMsg(ref e) => Some(e),
            TrackErr::IpcConn(ref e) => Some(e),
        }
    }
}

impl fmt::Display for TrackErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            TrackErr::Io(ref e) => write!(f, "io error: {}", e),
            TrackErr::Csv(ref e) => write!(f, "csv error: {}", e),
            TrackErr::Xcb(ref e) => write!(f, "xcb connection error: {}", e),
            TrackErr::IpcMsg(ref e) => write!(f, "i3ipc message error: {}", e),
            TrackErr::IpcConn(ref e) => write!(f, "i3ipc connection establishment error: {}", e),
        }
    }
}