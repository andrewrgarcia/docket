use std::fmt;
use std::io;
use std::path::Path;

pub type Result<T> = std::result::Result<T, Error>;

/// Every failure the user can hit, phrased as one line they can act on.
#[derive(Debug)]
pub enum Error {
    /// Wrong invocation. The message names the correct form.
    Usage(String),
    /// No card matched a name or prefix.
    NoCard(String),
    /// A prefix matched several cards.
    Ambiguous { query: String, hits: Vec<String> },
    /// The card already exists.
    Exists(String),
    /// A filesystem operation failed, with the path that failed.
    Io { doing: String, path: String, source: io::Error },
    /// Anything else worth one sentence.
    Other(String),
}

impl Error {
    pub fn io(doing: &str, path: &Path, source: io::Error) -> Self {
        Error::Io { doing: doing.into(), path: path.display().to_string(), source }
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }

    pub fn usage(msg: impl Into<String>) -> Self {
        Error::Usage(msg.into())
    }

    /// Distinct codes so scripts can branch: 2 is misuse, 3 is a missing card.
    pub fn code(&self) -> i32 {
        match self {
            Error::Usage(_) => 2,
            Error::NoCard(_) | Error::Ambiguous { .. } => 3,
            Error::Exists(_) => 4,
            Error::Io { .. } | Error::Other(_) => 1,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage(m) => write!(f, "{m}"),
            Error::NoCard(name) => {
                write!(f, "no card `{name}` — run `dk` to see them all")
            }
            Error::Ambiguous { query, hits } => {
                write!(f, "`{query}` matches {}", hits.join(", "))
            }
            Error::Exists(name) => {
                write!(f, "`{name}` already exists — try `dk edit {name}`")
            }
            Error::Io { doing, path, source } => {
                write!(f, "cannot {doing} {path}: {source}")
            }
            Error::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
