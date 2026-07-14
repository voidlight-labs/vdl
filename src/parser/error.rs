use crate::error::SourceLocation;
use thiserror::Error;

/// Errors that can occur during parsing.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ParseError {
    #[error("Unexpected token at {location}: {message}")]
    UnexpectedToken {
        location: SourceLocation,
        message: String,
    },

    #[error("Missing required field '{field}' at {location}")]
    MissingField {
        field: String,
        location: SourceLocation,
    },

    #[error("Invalid syntax at {location}: {message}")]
    InvalidSyntax {
        location: SourceLocation,
        message: String,
    },
}
