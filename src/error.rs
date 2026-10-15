use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// The file is not a GBX file at all.
    NotAGbx(String),
    /// Ran out of bytes while reading.
    Eof {
        what: String,
        at: usize,
    },
    /// Data did not have the expected shape.
    Invalid {
        what: String,
        at: usize,
        detail: String,
    },
    /// LZO decompression failed.
    Lzo(String),
    /// zlib inflate failed.
    Zlib(String),
    Io(std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotAGbx(m) => write!(f, "not a GBX file: {m}"),
            Error::Eof { what, at } => {
                write!(
                    f,
                    "unexpected end of data while reading {what} at offset {at}"
                )
            }
            Error::Invalid { what, at, detail } => {
                write!(
                    f,
                    "invalid data while reading {what} at offset {at}: {detail}"
                )
            }
            Error::Lzo(m) => write!(f, "LZO decompression failed: {m}"),
            Error::Zlib(m) => write!(f, "zlib decompression failed: {m}"),
            Error::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl Error {
    pub fn eof(what: impl Into<String>, at: usize) -> Self {
        Error::Eof {
            what: what.into(),
            at,
        }
    }

    pub fn invalid(what: impl Into<String>, at: usize, detail: impl Into<String>) -> Self {
        Error::Invalid {
            what: what.into(),
            at,
            detail: detail.into(),
        }
    }
}
