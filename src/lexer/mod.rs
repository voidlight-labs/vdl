pub mod token;

use crate::error::{SourceLocation, VdlError, VdlResult};
use crate::lexer::token::Token;
use std::path::Path;

/// A source span tracking byte offset, line, and column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }

    /// Convert this span to a [`SourceLocation`] using the given file path.
    pub fn to_location(&self, file: &Path) -> SourceLocation {
        SourceLocation::new(file, self.line, self.column)
    }
}

/// Advance the cursor by one byte, updating line and column tracking.
fn advance_char(bytes: &[u8], i: &mut usize, line: &mut usize, col: &mut usize) {
    if bytes[*i] == b'\n' {
        *line += 1;
        *col = 1;
    } else {
        *col += 1;
    }
    *i += 1;
}

/// Build a lexer error at the given position.
fn lexer_error(file: &Path, line: usize, col: usize, message: impl Into<String>) -> VdlError {
    VdlError::Lexer {
        location: SourceLocation::new(file, line, col),
        message: message.into(),
    }
}

/// Read a double-quoted string literal starting at the current position.
///
/// Assumes `bytes[*i] == b'"'`. Returns the unescaped string value and its span.
fn read_string_literal(
    bytes: &[u8],
    i: &mut usize,
    line: &mut usize,
    col: &mut usize,
    file: &Path,
) -> VdlResult<(String, Span)> {
    let start = *i;
    let start_line = *line;
    let start_col = *col;
    let len = bytes.len();

    advance_char(bytes, i, line, col); // consume opening quote

    let mut value = String::new();
    let mut closed = false;

    while *i < len {
        if bytes[*i] == b'"' {
            advance_char(bytes, i, line, col);
            closed = true;
            break;
        }

        if bytes[*i] == b'\\' {
            advance_char(bytes, i, line, col);
            if *i >= len {
                return Err(lexer_error(
                    file,
                    start_line,
                    start_col,
                    "unterminated string literal",
                ));
            }
            match bytes[*i] {
                b'\\' => value.push('\\'),
                b'"' => value.push('"'),
                b'n' => value.push('\n'),
                b't' => value.push('\t'),
                other => {
                    return Err(lexer_error(
                        file,
                        *line,
                        *col,
                        format!("unknown escape sequence: \\ {}", other as char),
                    ));
                }
            }
            advance_char(bytes, i, line, col);
        } else if bytes[*i].is_ascii() {
            if bytes[*i] == b'\n' {
                value.push('\n');
            } else {
                value.push(bytes[*i] as char);
            }
            advance_char(bytes, i, line, col);
        } else {
            // Multi-byte UTF-8 sequence: decode the whole sequence at once.
            let width = utf8_sequence_len(bytes[*i]).ok_or_else(|| {
                lexer_error(
                    file,
                    *line,
                    *col,
                    "invalid UTF-8 sequence in string literal",
                )
            })?;
            if *i + width > len {
                return Err(lexer_error(
                    file,
                    *line,
                    *col,
                    "truncated UTF-8 sequence in string literal",
                ));
            }
            let chunk = std::str::from_utf8(&bytes[*i..*i + width]).map_err(|_| {
                lexer_error(
                    file,
                    *line,
                    *col,
                    "invalid UTF-8 sequence in string literal",
                )
            })?;
            value.push_str(chunk);
            for _ in 0..width {
                advance_char(bytes, i, line, col);
            }
        }
    }

    if !closed {
        return Err(lexer_error(
            file,
            start_line,
            start_col,
            "unterminated string literal",
        ));
    }

    Ok((value, Span::new(start, *i, start_line, start_col)))
}

/// Byte width of a UTF-8 sequence starting with `byte`, or `None` when the
/// byte cannot start a valid sequence.
fn utf8_sequence_len(byte: u8) -> Option<usize> {
    match byte {
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

/// Read a raw triple-quoted body literal starting at the current position.
///
/// Assumes `bytes[*i] == b'"'` and the next two bytes are also `b'"'`.
///
/// Documented literal rules applied here:
/// 1. `\r\n` and lone `\r` are normalized to `\n`.
/// 2. No escape processing: `\"`, `\\` and friends stay literal.
/// 3. One newline immediately after the opening `"""` is dropped.
/// 4. A trailing whitespace-only line before the closing `"""` is dropped.
///
/// Indentation dedenting happens later, at parse time, because it depends on
/// the whole body being known.
fn read_raw_string_literal(
    bytes: &[u8],
    i: &mut usize,
    line: &mut usize,
    col: &mut usize,
    file: &Path,
) -> VdlResult<(String, Span)> {
    let start = *i;
    let start_line = *line;
    let start_col = *col;
    let len = bytes.len();

    // consume opening `"""`
    advance_char(bytes, i, line, col);
    advance_char(bytes, i, line, col);
    advance_char(bytes, i, line, col);

    let content_start = *i;
    let mut content_end = None;

    while *i + 2 < len {
        if bytes[*i] == b'"' && bytes[*i + 1] == b'"' && bytes[*i + 2] == b'"' {
            content_end = Some(*i);
            advance_char(bytes, i, line, col);
            advance_char(bytes, i, line, col);
            advance_char(bytes, i, line, col);
            break;
        }
        advance_char(bytes, i, line, col);
    }

    let content_end = match content_end {
        Some(end) => end,
        None => {
            return Err(lexer_error(
                file,
                start_line,
                start_col,
                "unterminated raw string literal",
            ))
        }
    };

    // Decode as UTF-8 so non-ASCII text survives intact. Pushing bytes as
    // chars (as `read_string_literal` does) would mangle multibyte input.
    let raw = std::str::from_utf8(&bytes[content_start..content_end]).map_err(|_| {
        lexer_error(
            file,
            start_line,
            start_col,
            "invalid UTF-8 in raw string literal",
        )
    })?;

    Ok((
        normalize_raw_body(raw),
        Span::new(start, *i, start_line, start_col),
    ))
}

fn normalize_raw_body(raw: &str) -> String {
    // Rule 1: normalize line endings.
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");

    // Rule 3: drop one newline right after the opening delimiter.
    let mut value = match normalized.strip_prefix('\n') {
        Some(rest) => rest.to_string(),
        None => normalized,
    };

    // Rule 4: drop a trailing whitespace-only line before the closing delimiter.
    if let Some(last_newline) = value.rfind('\n') {
        if value[last_newline + 1..].trim().is_empty() {
            value.truncate(last_newline);
        }
    }

    value
}

fn braced_body_field(tokens: &[(Token, Span)], depth: usize) -> Option<&Span> {
    if depth != 1 {
        return None;
    }
    match tokens {
        [.., (Token::Field, span), (Token::String(_), _)] => Some(span),
        // A legacy evidence block has the same opening syntax. Its keyword
        // still works as a schema field through the explicit field spelling.
        [.., (token, span)] if token.word().is_some() && *token != Token::Evidence => Some(span),
        _ => None,
    }
}

/// A braced body ends at a line containing only a closing brace aligned with
/// the field. All other braces belong to the raw Markdown.
fn read_braced_body(
    source: &str,
    start: usize,
    field: &Span,
    file: &Path,
    allow_comma: bool,
) -> VdlResult<(String, usize)> {
    let line_start = source[..field.start].rfind('\n').map_or(0, |i| i + 1);
    let prefix = &source[line_start..field.start];
    let indent = &prefix[..prefix.len() - prefix.trim_start_matches([' ', '\t']).len()];
    let after_open = &source[start + 1..];
    let (opening_line, rest) = after_open
        .split_once('\n')
        .filter(|(line, _)| line.trim_matches([' ', '\t', '\r']).is_empty())
        .ok_or_else(|| {
            lexer_error(file, field.line, field.column, "a braced body must start on the next line")
        })?;

    let content_start = start + 1 + opening_line.len();
    let mut offset = content_start + 1;
    for line in rest.split_inclusive('\n') {
        let closing = line.strip_prefix(indent).is_some_and(|line| {
            let line = line.trim_end_matches([' ', '\t', '\r', '\n']);
            line == "}" || (allow_comma && line == "},")
        });
        if closing {
            let end = offset + indent.len();
            return Ok((normalize_raw_body(&source[content_start..end]), end + 1));
        }
        offset += line.len();
    }
    Err(lexer_error(
        file,
        field.line,
        field.column,
        "unterminated braced body; expected a closing brace aligned with the field",
    ))
}

/// Tokenize VDL source code into a stream of tokens with span information.
///
/// # Arguments
///
/// * `source` — The raw VDL source text.
/// * `file` — The path to the source file (used for error locations).
///
/// # Errors
///
/// Returns [`VdlError::Lexer`] for unrecognized characters, unterminated strings,
/// or other lexical issues.
pub fn lex(source: &str, file: &Path) -> VdlResult<Vec<(Token, Span)>> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;
    let mut tokens = Vec::new();
    let mut depth = 0usize;

    while i < len {
        let start = i;
        let start_line = line;
        let start_col = col;
        let c = bytes[i];

        // --- Skip whitespace ---
        if c.is_ascii_whitespace() {
            advance_char(bytes, &mut i, &mut line, &mut col);
            continue;
        }

        // --- Line comment: // to end of line ---
        if c == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            i += 2;
            col += 2;
            while i < len && bytes[i] != b'\n' {
                i += 1;
                col += 1;
            }
            continue;
        }

        // --- Block comment: /* ... */ ---
        if c == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
            i += 2;
            col += 2;
            let mut closed = false;
            while i < len {
                if bytes[i] == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                    i += 2;
                    col += 2;
                    closed = true;
                    break;
                }
                if bytes[i] == b'\n' {
                    line += 1;
                    col = 1;
                    i += 1;
                } else {
                    i += 1;
                    col += 1;
                }
            }
            if !closed {
                return Err(lexer_error(
                    file,
                    start_line,
                    start_col,
                    "unterminated block comment",
                ));
            }
            continue;
        }

        // --- Raw body literal: """ ... """ ---
        if c == b'"' && i + 2 < len && bytes[i + 1] == b'"' && bytes[i + 2] == b'"' {
            let (s, span) = read_raw_string_literal(bytes, &mut i, &mut line, &mut col, file)?;
            tokens.push((Token::RawString(s), span));
            continue;
        }

        // --- String literal ---
        if c == b'"' {
            let (s, span) = read_string_literal(bytes, &mut i, &mut line, &mut col, file)?;
            tokens.push((Token::String(s), span));
            continue;
        }

        // --- Annotation: @identifier("value") ---
        if c == b'@' {
            advance_char(bytes, &mut i, &mut line, &mut col); // consume '@'
            let name_start = i;
            if i >= len || !(bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
                return Err(lexer_error(file, line, col, "expected identifier after @"));
            }
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                advance_char(bytes, &mut i, &mut line, &mut col);
            }
            let name = String::from_utf8_lossy(&bytes[name_start..i]).into_owned();

            // Expect '('
            if i >= len || bytes[i] != b'(' {
                return Err(lexer_error(
                    file,
                    line,
                    col,
                    format!("expected '(' after @ {}", name),
                ));
            }
            advance_char(bytes, &mut i, &mut line, &mut col);

            // Expect string literal
            if i >= len || bytes[i] != b'"' {
                return Err(lexer_error(
                    file,
                    line,
                    col,
                    "expected string literal in annotation",
                ));
            }
            let (value, _) = read_string_literal(bytes, &mut i, &mut line, &mut col, file)?;

            // Expect ')'
            if i >= len || bytes[i] != b')' {
                return Err(lexer_error(
                    file,
                    line,
                    col,
                    format!("expected ')' after string in @ {}", name),
                ));
            }
            advance_char(bytes, &mut i, &mut line, &mut col);

            tokens.push((
                Token::Annotation(name, value),
                Span::new(start, i, start_line, start_col),
            ));
            continue;
        }

        // --- Keywords and names ---
        if c.is_ascii_alphanumeric() || c == b'_' {
            let id_start = i;
            while i < len
                && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'-' | b'.'))
            {
                advance_char(bytes, &mut i, &mut line, &mut col);
            }
            let word = &source[id_start..i];
            let token = match word {
                // Entity type keywords
                "axiom" => Token::Axiom,
                "framework" => Token::Framework,
                "law" => Token::Law,
                "principle" => Token::Principle,
                "concept" => Token::Concept,
                "artifact" => Token::Artifact,
                "pillar" => Token::Pillar,
                "document" => Token::Document,
                "project" => Token::Project,
                "release" => Token::Release,
                "persona" => Token::Persona,
                "collection" => Token::Collection,
                // Property keywords
                "version" => Token::Version,
                "title" => Token::Title,
                "description" => Token::Description,
                "previous" => Token::Previous,
                // Relationship keywords
                "requires" => Token::Requires,
                "enables" => Token::Enables,
                "references" => Token::References,
                "based_on" => Token::BasedOn,
                "derives_from" => Token::DerivesFrom,
                "implements" => Token::Implements,
                "inspired_by" => Token::InspiredBy,
                "evolved_from" => Token::EvolvedFrom,
                "contradicts" => Token::Contradicts,
                // Evidence keywords
                "evidence" => Token::Evidence,
                "revelation" => Token::Revelation,
                "synthesis" => Token::Synthesis,
                "analogy" => Token::Analogy,
                "source" => Token::Source,
                "text" => Token::Text,
                "translator" => Token::Translator,
                "sources" => Token::Sources,
                "argument" => Token::Argument,
                "domain" => Token::Domain,
                "mapping" => Token::Mapping,
                // Schema document keywords
                "schema" => Token::Schema,
                "type" => Token::Type,
                "field" => Token::Field,
                "ref" => Token::Ref,
                "compose" => Token::Compose,
                "inline" => Token::Inline,
                "cite" => Token::Cite,
                "prose" => Token::Prose,
                "label" => Token::Label,
                "body" => Token::Body,
                "heading" => Token::Heading,
                "optional" => Token::Optional,
                "entity" => Token::Entity,
                "package" => Token::Package,
                "import" => Token::Import,
                "as" => Token::As,
                _ => Token::Identifier(word.to_owned()),
            };
            tokens.push((token, Span::new(start, i, start_line, start_col)));
            continue;
        }

        // --- Braced Markdown body ---
        if c == b'{' {
            if let Some(field) = braced_body_field(&tokens, depth) {
                let allow_comma = matches!(tokens.last(), Some((Token::Prose, _)));
                let (body, end) = read_braced_body(source, i, field, file, allow_comma)?;
                while i < end {
                    advance_char(bytes, &mut i, &mut line, &mut col);
                }
                tokens.push((
                    Token::BracedBody(body),
                    Span::new(start, i, start_line, start_col),
                ));
                continue;
            }
        }

        // --- Delimiters ---
        let token = match c {
            b'{' => {
                depth += 1;
                Token::LBrace
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                Token::RBrace
            }
            b'[' => Token::LBracket,
            b']' => Token::RBracket,
            b',' => Token::Comma,
            b'*' => Token::Star,
            _ => {
                return Err(lexer_error(
                    file,
                    start_line,
                    start_col,
                    format!("unknown character: '{}'", c as char),
                ));
            }
        };
        advance_char(bytes, &mut i, &mut line, &mut col);
        tokens.push((token, Span::new(start, i, start_line, start_col)));
    }

    Ok(tokens)
}

// =============================================================================
// Unit tests
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn test_file() -> &'static Path {
        Path::new("test.vdl")
    }

    // Helper: extract only tokens, ignoring spans.
    fn tokens(result: VdlResult<Vec<(Token, Span)>>) -> Vec<Token> {
        result.unwrap().into_iter().map(|(t, _)| t).collect()
    }

    // -------------------------------------------------------------------------
    // Schema document tokens
    // -------------------------------------------------------------------------

    #[test]
    fn lex_schema_keywords() {
        let src = r#"schema "s" { type "T" { field "f" text ref "T" compose body heading } entity "T" "i" { field "f" "v" } }"#;
        let toks = tokens(lex(src, test_file()));
        assert!(toks.contains(&Token::Schema));
        assert!(toks.contains(&Token::Type));
        assert_eq!(toks.iter().filter(|t| **t == Token::Field).count(), 2);
        assert!(toks.contains(&Token::Ref));
        assert!(toks.contains(&Token::Compose));
        assert!(toks.contains(&Token::Body));
        assert!(toks.contains(&Token::Heading));
        assert!(toks.contains(&Token::Entity));
    }

    #[test]
    fn raw_string_is_literal() {
        let src = "\"\"\"a \" b \\ c\"\"\"";
        let toks = tokens(lex(src, test_file()));
        assert_eq!(toks, vec![Token::RawString(r#"a " b \ c"#.to_string())]);
    }

    #[test]
    fn raw_string_trims_one_leading_newline() {
        let src = "\"\"\"\nX\n\"\"\"";
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::RawString("X".to_string())]
        );
    }

    #[test]
    fn raw_string_keeps_interior_blank_lines() {
        let src = "\"\"\"\nA\n\n\nB\n\"\"\"";
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::RawString("A\n\n\nB".to_string())]
        );
    }

    #[test]
    fn raw_string_keeps_non_ascii() {
        let src = "\"\"\"tafsir — “kutip”\"\"\"";
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::RawString("tafsir — “kutip”".to_string())]
        );
    }

    #[test]
    fn raw_string_normalizes_crlf() {
        let src = "\"\"\"\r\nA\r\nB\r\n\"\"\"";
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::RawString("A\nB".to_string())]
        );
    }

    #[test]
    fn braced_body_normalizes_crlf_and_accepts_explicit_field_names() {
        let source = "entity \"Page\" \"page\" {\r\n\
                      \x20   field \"evidence\" {\r\n\
                      \x20       Rūḥ {signal} \"\"\"\r\n\
                      \x20   }\r\n\
                      }";
        let tokens = tokens(lex(source, test_file()));
        assert!(tokens.contains(&Token::BracedBody("        Rūḥ {signal} \"\"\"".into())));
        assert_eq!(tokens.last(), Some(&Token::RBrace));
    }

    #[test]
    fn braced_body_reports_an_unaligned_closing_brace_at_the_field() {
        let source = "artifact \"page\" {\n    body {\n        prose\n}";
        let error = lex(source, test_file()).unwrap_err();
        match error {
            VdlError::Lexer { location, message } => {
                assert_eq!((location.line, location.column), (2, 5));
                assert!(message.contains("closing brace aligned with the field"));
            }
            other => panic!("expected lexer error, got {other:?}"),
        }
    }

    #[test]
    fn raw_string_unterminated_is_error() {
        let src = "\"\"\"never closed";
        assert!(matches!(
            lex(src, test_file()),
            Err(VdlError::Lexer { .. })
        ));
    }

    #[test]
    fn string_literal_preserves_utf8() {
        let src = "\"Naskah — “kutip” ✓\"";
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::String("Naskah — “kutip” ✓".to_string())]
        );
    }

    // -------------------------------------------------------------------------
    // Keyword coverage
    // -------------------------------------------------------------------------
    #[test]
    fn entity_keywords() {
        let src = "axiom framework law principle concept artifact pillar document project release persona collection";
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![
                Token::Axiom,
                Token::Framework,
                Token::Law,
                Token::Principle,
                Token::Concept,
                Token::Artifact,
                Token::Pillar,
                Token::Document,
                Token::Project,
                Token::Release,
                Token::Persona,
                Token::Collection,
            ]
        );
    }

    #[test]
    fn property_keywords() {
        let src = "version title description previous";
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::Version, Token::Title, Token::Description, Token::Previous,]
        );
    }

    #[test]
    fn relationship_keywords() {
        let src = "requires enables references based_on derives_from implements inspired_by evolved_from contradicts";
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![
                Token::Requires,
                Token::Enables,
                Token::References,
                Token::BasedOn,
                Token::DerivesFrom,
                Token::Implements,
                Token::InspiredBy,
                Token::EvolvedFrom,
                Token::Contradicts,
            ]
        );
    }

    #[test]
    fn evidence_keywords() {
        let src = "evidence revelation synthesis analogy source text translator sources argument domain mapping";
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![
                Token::Evidence,
                Token::Revelation,
                Token::Synthesis,
                Token::Analogy,
                Token::Source,
                Token::Text,
                Token::Translator,
                Token::Sources,
                Token::Argument,
                Token::Domain,
                Token::Mapping,
            ]
        );
    }

    // Ensure longest keyword wins (sources vs source).
    #[test]
    fn longest_keyword_wins() {
        let src = "sources source";
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::Sources, Token::Source]
        );
    }

    // -------------------------------------------------------------------------
    // String literals
    // -------------------------------------------------------------------------
    #[test]
    fn string_literal_simple() {
        let src = r#""hello world""#;
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::String("hello world".into())]
        );
    }

    #[test]
    fn string_literal_escapes() {
        let src = r#""quote: \" and backslash: \\ " "#;
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::String("quote: \" and backslash: \\ ".into())]
        );
    }

    #[test]
    fn string_literal_multiline() {
        let src = "\"line one\nline two\nline three\"";
        let toks = tokens(lex(src, test_file()));
        assert_eq!(toks.len(), 1);
        if let Token::String(s) = &toks[0] {
            assert_eq!(s, "line one\nline two\nline three");
        } else {
            panic!("expected string token");
        }
    }

    #[test]
    fn string_literal_with_newline_escape() {
        let src = r#""hello\nworld""#;
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::String("hello\nworld".into())]
        );
    }

    #[test]
    fn string_literal_with_tab_escape() {
        let src = r#""hello\tworld""#;
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::String("hello\tworld".into())]
        );
    }

    // -------------------------------------------------------------------------
    // Annotations
    // -------------------------------------------------------------------------
    #[test]
    fn annotation_simple() {
        let src = r#"@stable("v1.0")"#;
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::Annotation("stable".into(), "v1.0".into())]
        );
    }

    #[test]
    fn annotation_with_escapes() {
        let src = r#"@note("say \"hello\"")"#;
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::Annotation("note".into(), "say \"hello\"".into())]
        );
    }

    // -------------------------------------------------------------------------
    // Comments
    // -------------------------------------------------------------------------
    #[test]
    fn line_comment_skipped() {
        let src = "axiom // this is a comment\nframework";
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::Axiom, Token::Framework]
        );
    }

    #[test]
    fn block_comment_skipped() {
        let src = "axiom /* block\ncomment */ framework";
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![Token::Axiom, Token::Framework]
        );
    }

    #[test]
    fn comment_at_end_of_file() {
        let src = "axiom // trailing comment";
        assert_eq!(tokens(lex(src, test_file())), vec![Token::Axiom]);
    }

    #[test]
    fn block_comment_at_end_of_file() {
        let src = "axiom /* trailing block */";
        assert_eq!(tokens(lex(src, test_file())), vec![Token::Axiom]);
    }

    // -------------------------------------------------------------------------
    // Delimiters
    // -------------------------------------------------------------------------
    #[test]
    fn delimiters() {
        let src = "{ } [ ] ,";
        assert_eq!(
            tokens(lex(src, test_file())),
            vec![
                Token::LBrace,
                Token::RBrace,
                Token::LBracket,
                Token::RBracket,
                Token::Comma,
            ]
        );
    }

    // -------------------------------------------------------------------------
    // Spans
    // -------------------------------------------------------------------------
    #[test]
    fn span_tracking() {
        let src = "axiom\n  framework";
        let result = lex(src, test_file()).unwrap();
        assert_eq!(result.len(), 2);

        // "axiom" starts at byte 0, line 1, col 1
        assert_eq!(result[0].1, Span::new(0, 5, 1, 1));
        // "framework" starts at byte 8, line 2, col 3
        assert_eq!(result[1].1, Span::new(8, 17, 2, 3));
    }

    // -------------------------------------------------------------------------
    // Error cases
    // -------------------------------------------------------------------------
    #[test]
    fn error_unterminated_string() {
        let src = r#""hello"#;
        let err = lex(src, test_file()).unwrap_err();
        match err {
            VdlError::Lexer { location: _location, message } => {
                assert!(message.contains("unterminated string literal"));
            }
            _ => panic!("expected lexer error"),
        }
    }

    #[test]
    fn lexes_schema_identifiers() {
        assert_eq!(
            tokens(lex("includes custom_type", test_file())),
            vec![
                Token::Identifier("includes".into()),
                Token::Identifier("custom_type".into()),
            ],
        );
    }

    #[test]
    fn error_unknown_character() {
        let src = "axiom #";
        let err = lex(src, test_file()).unwrap_err();
        match err {
            VdlError::Lexer { location: _location, message } => {
                assert!(message.contains("unknown character: '#'"));
            }
            _ => panic!("expected lexer error"),
        }
    }

    #[test]
    fn error_unterminated_block_comment() {
        let src = "axiom /* unterminated";
        let err = lex(src, test_file()).unwrap_err();
        match err {
            VdlError::Lexer { location: _location, message } => {
                assert!(message.contains("unterminated block comment"));
            }
            _ => panic!("expected lexer error"),
        }
    }

    #[test]
    fn error_unknown_escape_in_string() {
        let src = r#""hello\xworld""#;
        let err = lex(src, test_file()).unwrap_err();
        match err {
            VdlError::Lexer { location: _location, message } => {
                assert!(message.contains("unknown escape sequence"));
            }
            _ => panic!("expected lexer error"),
        }
    }

    // -------------------------------------------------------------------------
    // Full integration
    // -------------------------------------------------------------------------
    #[test]
    fn mixed_tokens() {
        let src = r#"
            axiom "My Axiom" {
                version "1.0"
                requires [ "Other" ]
            }
        "#;
        let toks = tokens(lex(src, test_file()));
        assert_eq!(
            toks,
            vec![
                Token::Axiom,
                Token::String("My Axiom".into()),
                Token::LBrace,
                Token::Version,
                Token::String("1.0".into()),
                Token::Requires,
                Token::LBracket,
                Token::String("Other".into()),
                Token::RBracket,
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn empty_source() {
        assert_eq!(tokens(lex("", test_file())), Vec::<Token>::new());
    }
}
