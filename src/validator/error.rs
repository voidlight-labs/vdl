use crate::error::SourceLocation;
use thiserror::Error;

/// Errors that can occur during validation.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ValidationError {
    #[error("Type constraint violation at {location}: {message}")]
    TypeConstraint {
        location: SourceLocation,
        message: String,
    },

    #[error("Reference integrity violation at {location}: {message}")]
    ReferenceIntegrity {
        location: SourceLocation,
        message: String,
    },

    #[error("Missing evidence at {location}: {message}")]
    MissingEvidence {
        location: SourceLocation,
        message: String,
    },

    #[error("Incomplete evidence at {location}: {message}")]
    IncompleteEvidence {
        location: SourceLocation,
        message: String,
    },

    #[error("Cycle detected: {message}")]
    Cycle { message: String },

    #[error("Invalid version format at {location}: {message}")]
    InvalidVersion {
        location: SourceLocation,
        message: String,
    },
}

impl From<ValidationError> for crate::error::VdlError {
    fn from(e: ValidationError) -> Self {
        let (location, message) = match e {
            ValidationError::TypeConstraint { location, message }
            | ValidationError::ReferenceIntegrity { location, message }
            | ValidationError::MissingEvidence { location, message }
            | ValidationError::IncompleteEvidence { location, message }
            | ValidationError::InvalidVersion { location, message } => (location, message),
            ValidationError::Cycle { message } => {
                let loc = SourceLocation::new(std::path::PathBuf::new(), 0, 0);
                (loc, message)
            }
        };
        crate::error::VdlError::Validation { location, message }
    }
}
