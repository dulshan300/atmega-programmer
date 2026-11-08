use serde::Serialize;
use std::path::PathBuf;

/// Error type shared by all services. Serialized to the frontend as
/// `{ "kind": "...", "message": "..." }`.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Invalid project: {0}")]
    InvalidProject(String),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Unknown device: {0}")]
    UnknownDevice(String),
    #[error("Toolchain error: {0}")]
    Toolchain(String),
    #[error("Process error: {0}")]
    Process(String),
    #[error("Serial port error: {0}")]
    Serial(String),
    #[error("Resource busy: {0}")]
    Busy(String),
    #[error("Refused for safety: {0}")]
    Safety(String),
    #[error("Conflict: {0}")]
    Conflict(String),
}

impl CoreError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        CoreError::Io {
            path: path.into(),
            source,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            CoreError::Io { .. } => "io",
            CoreError::InvalidProject(_) => "invalidProject",
            CoreError::InvalidRequest(_) => "invalidRequest",
            CoreError::UnknownDevice(_) => "unknownDevice",
            CoreError::Toolchain(_) => "toolchain",
            CoreError::Process(_) => "process",
            CoreError::Serial(_) => "serial",
            CoreError::Busy(_) => "busy",
            CoreError::Safety(_) => "safety",
            CoreError::Conflict(_) => "conflict",
        }
    }
}

impl Serialize for CoreError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("CoreError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type CoreResult<T> = Result<T, CoreError>;
