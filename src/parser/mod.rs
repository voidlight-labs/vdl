pub mod schema;

use crate::error::{SourceLocation, VdlError, VdlResult};
use crate::lexer::Span;
use std::path::Path;

// ---------------------------------------------------------------------------
// Bridge: chumsky 0.9 requires its own `Span` trait on the stream's span type.
// Our `Span` already tracks byte offsets, so we implement the trait directly.
// ---------------------------------------------------------------------------
impl chumsky::span::Span for Span {
    type Context = ();
    type Offset = usize;
    fn start(&self) -> Self::Offset {
        self.start
    }
    fn end(&self) -> Self::Offset {
        self.end
    }
    fn context(&self) -> Self::Context {
        ()
    }
    fn new(_context: Self::Context, range: std::ops::Range<Self::Offset>) -> Self {
        Span::new(range.start, range.end, 0, 0)
    }
}

/// Convert a byte offset in source text to 1-indexed line and column.
fn byte_offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Build a [`SourceLocation`] from a span, falling back to computing line/col
/// from the source text when chumsky's merged spans have lost them.
pub(crate) fn span_to_location(span: Span, source: &str, file: &Path) -> SourceLocation {
    let (line, col) = if span.line > 0 {
        (span.line, span.column)
    } else {
        byte_offset_to_line_col(source, span.start)
    };
    SourceLocation::new(file, line, col)
}

// ---------------------------------------------------------------------------
// Legacy form detection
// ---------------------------------------------------------------------------

/// Entity-type keywords of the removed v0.1 fixed-type form.
///
/// The legacy pipeline (fixed entity types, evidence blocks, graph builder,
/// JSON/DOT/search/markdown outputs) was removed on 2026-11-16. These names
/// survive only here, so documents still written in the legacy form get a
/// clear "no longer supported" error instead of a confusing parse failure.
/// Schema-form shorthand instances may reuse these words as author-defined
/// type names, so they are *not* reserved by the lexer.
const LEGACY_ENTITY_TYPES: [&str; 12] = [
    "axiom", "framework", "law", "principle", "concept", "artifact", "pillar", "document",
    "project", "release", "persona", "collection",
];

/// Heuristic: does this token stream look like a removed legacy (v0.1)
/// document?
///
/// The old grammar started with a fixed entity type and a quoted ID. This is
/// ambiguous with schema shorthand, so the project loader first checks the
/// declared type names and gives those precedence. Single-file documents must
/// have a schema block.
pub(crate) fn is_legacy_document(tokens: &[(crate::lexer::token::Token, Span)]) -> bool {
    use crate::lexer::token::Token;

    let Some(first) = schema::first_declaration(tokens) else {
        return false;
    };
    let Some(word) = first.word() else {
        return false;
    };
    if !LEGACY_ENTITY_TYPES.contains(&word) {
        return false;
    }
    let Some(index) = tokens.iter().position(|(token, _)| std::ptr::eq(token, first)) else {
        return false;
    };
    matches!(tokens.get(index + 1), Some((Token::String(_), _)))
        && !schema::declares_schema(tokens)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse VDL source into a schema document.
///
/// # Errors
///
/// Returns [`VdlError::Other`] when the document is written in the removed
/// legacy (v0.1 fixed-type) form, and [`VdlError::Parser`] when the document
/// does not contain exactly one schema block or does not match the grammar.
pub fn parse_document(
    tokens: &[(crate::lexer::token::Token, Span)],
    source: &str,
    file: &Path,
) -> VdlResult<crate::model::SchemaDoc> {
    if is_legacy_document(tokens) {
        return Err(VdlError::Other {
            message: "legacy form is no longer supported: this document uses the removed v0.1 \
                  fixed-type form (fixed entity types, evidence blocks). Migrate it to the \
                  schema form: declare a `schema \"...\"` block with author-defined types and \
                  rewrite the entities as instances. See docs/grammar.md."
                .to_string(),
        });
    }
    schema::parse_schema_document(tokens, source, file)
}
