use serde::Serialize;

/// Why an archive could not be read. Maps onto `containers.status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveFailure {
    MissingParts(String),
    Encrypted,
    Corrupt(String),
    Unsupported(String),
    TooLarge(u64),
    ToolMissing,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{}", describe_archive(.0))]
    Archive(ArchiveFailure),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("Cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

fn describe_archive(f: &ArchiveFailure) -> String {
    match f {
        ArchiveFailure::MissingParts(d) => d.clone(),
        ArchiveFailure::Encrypted => "Archive is password protected".into(),
        ArchiveFailure::Corrupt(d) => format!("Archive is damaged: {d}"),
        ArchiveFailure::Unsupported(d) => format!("Unsupported archive: {d}"),
        ArchiveFailure::TooLarge(n) => {
            format!("Entry is too large to preview ({} MB)", n / (1024 * 1024))
        }
        ArchiveFailure::ToolMissing => {
            "7-Zip was not found, so 7z and rar archives cannot be read".into()
        }
    }
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Db(_) => "db",
            AppError::Io(_) => "io",
            AppError::Archive(ArchiveFailure::MissingParts(_)) => "missing_parts",
            AppError::Archive(ArchiveFailure::Encrypted) => "encrypted",
            AppError::Archive(ArchiveFailure::TooLarge(_)) => "too_large",
            AppError::Archive(ArchiveFailure::ToolMissing) => "tool_missing",
            AppError::Archive(_) => "archive",
            AppError::NotFound(_) => "not_found",
            AppError::Invalid(_) => "invalid",
            AppError::Cancelled => "cancelled",
            AppError::Other(_) => "other",
        }
    }
}

impl From<zip::result::ZipError> for AppError {
    fn from(e: zip::result::ZipError) -> Self {
        use zip::result::ZipError as Z;
        match e {
            Z::Io(io) => AppError::Io(io),
            Z::UnsupportedArchive(m) if m.to_lowercase().contains("password") => {
                AppError::Archive(ArchiveFailure::Encrypted)
            }
            Z::UnsupportedArchive(m) => {
                AppError::Archive(ArchiveFailure::Unsupported(m.to_string()))
            }
            Z::InvalidPassword => AppError::Archive(ArchiveFailure::Encrypted),
            Z::FileNotFound => AppError::NotFound("entry not found in archive".into()),
            other => AppError::Archive(ArchiveFailure::Corrupt(other.to_string())),
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Other(format!("JSON error: {e}"))
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::Other(e.to_string())
    }
}

#[derive(Serialize)]
struct WireError<'a> {
    kind: &'a str,
    message: String,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WireError {
            kind: self.kind(),
            message: self.to_string(),
        }
        .serialize(s)
    }
}

pub type AppResult<T> = Result<T, AppError>;
