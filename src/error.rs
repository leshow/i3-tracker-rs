use csv;
use std::{error::Error, fmt, io, time};
use xdg;

#[derive(Debug)]
pub enum TrackErr {
    Io(io::Error),
    Csv(csv::Error),
    BaseDir(xdg::BaseDirectoriesError),
    TimeErr(time::SystemTimeError),
}

impl From<csv::Error> for TrackErr {
    fn from(e: csv::Error) -> TrackErr {
        TrackErr::Csv(e)
    }
}

impl From<io::Error> for TrackErr {
    fn from(e: io::Error) -> TrackErr {
        TrackErr::Io(e)
    }
}

impl From<xdg::BaseDirectoriesError> for TrackErr {
    fn from(e: xdg::BaseDirectoriesError) -> TrackErr {
        TrackErr::BaseDir(e)
    }
}

impl From<time::SystemTimeError> for TrackErr {
    fn from(e: time::SystemTimeError) -> TrackErr {
        TrackErr::TimeErr(e)
    }
}

impl Error for TrackErr {
    fn description(&self) -> &str {
        match *self {
            TrackErr::Io(ref e) => e.description(),
            TrackErr::Csv(ref e) => e.description(),
            TrackErr::BaseDir(ref e) => e.description(),
            TrackErr::TimeErr(ref e) => e.description(),
        }
    }

    fn cause(&self) -> Option<&dyn Error> {
        match *self {
            TrackErr::Io(ref e) => Some(e),
            TrackErr::Csv(ref e) => Some(e),
            TrackErr::BaseDir(ref e) => Some(e),
            TrackErr::TimeErr(ref e) => Some(e),
        }
    }
}

impl fmt::Display for TrackErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            TrackErr::Io(ref e) => write!(f, "io error: {:#?}", e),
            TrackErr::Csv(ref e) => write!(f, "csv error: {:#?}", e),
            TrackErr::BaseDir(ref e) => write!(f, "XDG dirs not found: {:#?}", e),
            TrackErr::TimeErr(ref e) => write!(f, "Can't get system time: {:#?}", e),
        }
    }
}
