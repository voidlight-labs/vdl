use crate::error::SourceLocation;
use thiserror::Error;

/// Errors that can occur during validation.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ValidationError {
    #[error("Schema violation at {location}: {message}")]
    Schema {
        location: SourceLocation,
        message: String,
    },
}

impl From<ValidationError> for crate::error::VdlError {
    fn from(e: ValidationError) -> Self {
        let ValidationError::Schema { location, message } = e;
        crate::error::VdlError::Validation { location, message }
    }
}
