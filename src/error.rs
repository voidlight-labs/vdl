use std::path::PathBuf;
use thiserror::Error;

/// A location within a VDL source file.
///
/// Used for precise error reporting with file path, line, and column.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceLocation {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
}

impl SourceLocation {
    pub fn new(file: impl Into<PathBuf>, line: usize, column: usize) -> Self {
        Self {
            file: file.into(),
            line,
            column,
        }
    }
}

impl std::fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.file.display(), self.line, self.column)
    }
}

/// The main error type for the VDL compiler.
///
/// Every variant carries sufficient context for rich diagnostic messages.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum VdlError {
    #[error("Lexical error at {location}: {message}")]
    Lexer {
        location: SourceLocation,
        message: String,
    },

    #[error("Parse error at {location}: {message}")]
    Parser {
        location: SourceLocation,
        message: String,
    },

    #[error("Validation error at {location}: {message}")]
    Validation {
        location: SourceLocation,
        message: String,
    },

    #[error("Validation failed with {count} error(s):\n{messages}")]
    ValidationErrors {
        count: usize,
        messages: String,
    },

    #[error("Graph error: {message}")]
    Graph { message: String },

    #[error("Codegen error: {message}")]
    Codegen { message: String },

    #[error("IO error: {message}")]
    Io { message: String },

    #[error("{message}")]
    Other { message: String },
}

/// Convenience type alias for VDL operation results.
pub type VdlResult<T> = Result<T, VdlError>;
