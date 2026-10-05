use serde::Serialize;
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationErrorCode {
    ItemNotFound,
    ResourceMissing,
    ResourceUnreadable,
    PermissionDenied,
    InvalidContent,
    UnsupportedContent,
    ImageDecodeFailed,
    ClipboardUnavailable,
    ClipboardWriteFailed,
    PasteFailed,
    SaveFailed,
    InvalidDestination,
    ExplorerFailed,
    Internal,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationError {
    pub code: OperationErrorCode,
    pub detail: String,
}

impl OperationError {
    pub fn new(code: OperationErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    pub fn database(error: rusqlite::Error) -> Self {
        let code = match &error {
            rusqlite::Error::QueryReturnedNoRows => OperationErrorCode::ItemNotFound,
            rusqlite::Error::SqliteFailure(error, _)
                if matches!(
                    error.code,
                    rusqlite::ErrorCode::PermissionDenied
                        | rusqlite::ErrorCode::ReadOnly
                        | rusqlite::ErrorCode::AuthorizationForStatementDenied
                ) =>
            {
                OperationErrorCode::PermissionDenied
            }
            _ => OperationErrorCode::Internal,
        };
        Self::new(code, format!("database operation: {error}"))
    }

    pub fn clipboard(
        stage: impl fmt::Display,
        error: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        #[cfg(windows)]
        let code = if error
            .downcast_ref::<clipboard_rs::ClipboardAccessError>()
            .is_some()
        {
            OperationErrorCode::ClipboardUnavailable
        } else if error
            .downcast_ref::<clipboard_rs::ClipboardFormatUnsupportedError>()
            .is_some()
        {
            OperationErrorCode::UnsupportedContent
        } else {
            OperationErrorCode::ClipboardWriteFailed
        };
        #[cfg(not(windows))]
        let code = OperationErrorCode::ClipboardWriteFailed;
        Self::new(code, format!("{stage}: {error}"))
    }

    pub fn io(stage: &str, error: std::io::Error, fallback: OperationErrorCode) -> Self {
        let code = match error.kind() {
            std::io::ErrorKind::NotFound => OperationErrorCode::ResourceMissing,
            std::io::ErrorKind::PermissionDenied => OperationErrorCode::PermissionDenied,
            _ => fallback,
        };
        Self::new(code, format!("{stage}: {error}"))
    }
}

impl fmt::Display for OperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.detail)
    }
}

impl std::error::Error for OperationError {}

/// Check resources without decoding previews or changing the source.
pub fn readable_resource(path: &Path) -> Result<std::fs::Metadata, OperationError> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        OperationError::io(
            "source metadata",
            error,
            OperationErrorCode::ResourceUnreadable,
        )
    })?;
    if metadata.is_file() {
        std::fs::File::open(path).map_err(|error| {
            OperationError::io("source read", error, OperationErrorCode::ResourceUnreadable)
        })?;
    } else if metadata.is_dir() {
        std::fs::read_dir(path).map_err(|error| {
            OperationError::io(
                "source directory",
                error,
                OperationErrorCode::ResourceUnreadable,
            )
        })?;
    } else {
        return Err(OperationError::new(
            OperationErrorCode::UnsupportedContent,
            "source is not a file or directory",
        ));
    }
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_reasons_and_wire_codes_are_stable() {
        let error = OperationError::io(
            "read",
            std::io::ErrorKind::PermissionDenied.into(),
            OperationErrorCode::SaveFailed,
        );
        assert_eq!(error.code, OperationErrorCode::PermissionDenied);
        let encoded = serde_json::to_value(error).unwrap();
        assert_eq!(encoded["code"], "permission_denied");
        assert!(encoded["detail"].as_str().unwrap().starts_with("read:"));
        assert_eq!(
            OperationError::io(
                "read",
                std::io::ErrorKind::NotFound.into(),
                OperationErrorCode::SaveFailed
            )
            .code,
            OperationErrorCode::ResourceMissing
        );
    }

    #[test]
    fn database_missing_and_permission_failures_have_explicit_reasons() {
        assert_eq!(
            OperationError::database(rusqlite::Error::QueryReturnedNoRows).code,
            OperationErrorCode::ItemNotFound
        );
        let readonly = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_READONLY),
            None,
        );
        assert_eq!(
            OperationError::database(readonly).code,
            OperationErrorCode::PermissionDenied
        );
    }

    #[test]
    fn clipboard_write_error_text_is_not_guessed_to_be_access_failure() {
        let error = OperationError::clipboard("write", "Open clipboard error, code = 5".into());
        assert_eq!(error.code, OperationErrorCode::ClipboardWriteFailed);
    }

    #[cfg(windows)]
    #[test]
    fn typed_native_unsupported_format_has_explicit_reason() {
        let error = OperationError::clipboard(
            "write",
            Box::new(clipboard_rs::ClipboardFormatUnsupportedError),
        );
        assert_eq!(error.code, OperationErrorCode::UnsupportedContent);
    }
}
