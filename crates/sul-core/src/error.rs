use std::path::PathBuf;

/// Crate-wide error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Scaffolded API that has not been built yet.
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),

    /// The feature cannot work on this platform or in this build configuration.
    #[error("unsupported here: {0}")]
    Unsupported(&'static str),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// JSON that did not come from a file on disk (a wire frame, an in-memory manifest).
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// A game identifier that is unsafe to use as a directory name.
    #[error("invalid game id {0:?}")]
    InvalidGameId(String),

    /// A template file written by a newer launcher than this one.
    #[error("template schema {found} is newer than supported schema {supported}")]
    TemplateTooNew { found: u32, supported: u32 },

    #[error("steam: {0}")]
    Steam(String),

    /// An unexpected Windows registry failure (missing keys and access-denied
    /// subkeys are skipped, not reported).
    #[error("registry error at {key}: {source}")]
    Registry {
        key: String,
        #[source]
        source: std::io::Error,
    },

    #[error("no game with id {0:?} in the library")]
    UnknownGame(String),

    /// Tried to tick a feature that is not applicable to, or known not to work with, a game.
    #[error("cannot enable {feature}: {reason}")]
    FeatureUnavailable {
        feature: &'static str,
        reason: &'static str,
    },

    /// A relative path that is empty, absolute, or climbs out of its root, or a symlink that escapes.
    #[error("unsafe path rejected: {0}")]
    UnsafePath(String),

    /// A write or delete aimed at a protected location (for example the original game install).
    #[error("refused: {0} is inside a protected location")]
    Protected(PathBuf),

    #[error("destination already exists: {0}")]
    DestinationExists(PathBuf),

    #[error("invalid argument: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn json(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        Self::Json {
            path: path.into(),
            source,
        }
    }
}

/// `map_err` adapter that attaches the path to an I/O failure:
/// `std::fs::read(p).map_err(io(p))`.
pub(crate) fn io<P: Into<PathBuf>>(path: P) -> impl FnOnce(std::io::Error) -> Error {
    let path = path.into();
    move |source| Error::Io { path, source }
}
