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

    // Helper closure to advance the cursor and update line/column tracking.
    let advance = |idx: &mut usize, l: &mut usize, c: &mut usize| {
        if bytes[*idx] == b'\n' {
            *l += 1;
            *c = 1;
        } else {
            *c += 1;
        }
        *idx += 1;
    };

    // Helper to build a lexer error at the current or given position.
    let mk_err = |l: usize, c: usize, msg: String| -> VdlError {
        VdlError::Lexer {
            location: SourceLocation::new(file, l, c),
            message: msg,
        }
    };

    while i < len {
        let start = i;
        let start_line = line;
        let start_col = col;
        let c = bytes[i];

        // --- Skip whitespace ---
        if c.is_ascii_whitespace() {
            advance(&mut i, &mut line, &mut col);
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
                return Err(mk_err(
                    start_line,
                    start_col,
                    "unterminated block comment".into(),
                ));
            }
            continue;
        }

        // --- String literal ---
        if c == b'"' {
            advance(&mut i, &mut line, &mut col); // consume opening quote
            let mut s = String::new();
            let mut closed = false;
            while i < len {
                if bytes[i] == b'"' {
                    advance(&mut i, &mut line, &mut col);
                    closed = true;
                    break;
                }
                if bytes[i] == b'\\' {
                    advance(&mut i, &mut line, &mut col);
                    if i >= len {
                        return Err(mk_err(
                            start_line,
                            start_col,
                            "unterminated string literal".into(),
                        ));
                    }
                    match bytes[i] {
                        b'\\' => s.push('\\'),
                        b'"' => s.push('"'),
                        b'n' => s.push('\n'),
                        b't' => s.push('\t'),
                        other => {
                            return Err(mk_err(
                                line,
                                col,
                                format!("unknown escape sequence: \\ {}", other as char),
                            ));
                        }
                    }
                    advance(&mut i, &mut line, &mut col);
                } else {
                    if bytes[i] == b'\n' {
                        s.push('\n');
                    } else {
                        s.push(bytes[i] as char);
                    }
                    advance(&mut i, &mut line, &mut col);
                }
            }
            if !closed {
                return Err(mk_err(
                    start_line,
                    start_col,
                    "unterminated string literal".into(),
                ));
            }
            tokens.push((Token::String(s), Span::new(start, i, start_line, start_col)));
            continue;
        }

        // --- Annotation: @identifier("value") ---
        if c == b'@' {
            advance(&mut i, &mut line, &mut col); // consume '@'
            let name_start = i;
            if i >= len || !(bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
                return Err(mk_err(line, col, "expected identifier after @".into()));
            }
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                advance(&mut i, &mut line, &mut col);
            }
            let name = String::from_utf8_lossy(&bytes[name_start..i]).into_owned();

            // Expect '('
            if i >= len || bytes[i] != b'(' {
                return Err(mk_err(
                    line,
                    col,
                    format!("expected '(' after @ {}", name),
                ));
            }
            advance(&mut i, &mut line, &mut col);

            // Expect string literal
            if i >= len || bytes[i] != b'"' {
                return Err(mk_err(
                    line,
                    col,
                    "expected string literal in annotation".into(),
                ));
            }
            let str_start_line = line;
            let str_start_col = col;
            advance(&mut i, &mut line, &mut col); // consume opening quote
            let mut value = String::new();
            let mut value_closed = false;
            while i < len {
                if bytes[i] == b'"' {
                    advance(&mut i, &mut line, &mut col);
                    value_closed = true;
                    break;
                }
                if bytes[i] == b'\\' {
                    advance(&mut i, &mut line, &mut col);
                    if i >= len {
                        return Err(mk_err(
                            str_start_line,
                            str_start_col,
                            "unterminated string literal in annotation".into(),
                        ));
                    }
                    match bytes[i] {
                        b'\\' => value.push('\\'),
                        b'"' => value.push('"'),
                        b'n' => value.push('\n'),
                        b't' => value.push('\t'),
                        other => {
                            return Err(mk_err(
                                line,
                                col,
                                format!("unknown escape sequence: \\ {}", other as char),
                            ));
                        }
                    }
                    advance(&mut i, &mut line, &mut col);
                } else {
                    if bytes[i] == b'\n' {
                        value.push('\n');
                    } else {
                        value.push(bytes[i] as char);
                    }
                    advance(&mut i, &mut line, &mut col);
                }
            }
            if !value_closed {
                return Err(mk_err(
                    str_start_line,
                    str_start_col,
                    "unterminated string literal in annotation".into(),
                ));
            }

            // Expect ')'
            if i >= len || bytes[i] != b')' {
                return Err(mk_err(
                    line,
                    col,
                    format!("expected ')' after string in @ {}", name),
                ));
            }
            advance(&mut i, &mut line, &mut col);

            tokens.push((
                Token::Annotation(name, value),
                Span::new(start, i, start_line, start_col),
            ));
            continue;
        }

        // --- Keywords and identifiers ---
        if c.is_ascii_alphabetic() || c == b'_' {
            let id_start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                advance(&mut i, &mut line, &mut col);
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
                // Unknown bare word → lex error
                _ => {
                    return Err(mk_err(
                        start_line,
                        start_col,
                        format!("unknown word: '{}'", word),
                    ));
                }
            };
            tokens.push((token, Span::new(start, i, start_line, start_col)));
            continue;
        }

        // --- Delimiters ---
        let token = match c {
            b'{' => Token::LBrace,
            b'}' => Token::RBrace,
            b'[' => Token::LBracket,
            b']' => Token::RBracket,
            b',' => Token::Comma,
            _ => {
                return Err(mk_err(
                    start_line,
                    start_col,
                    format!("unknown character: '{}'", c as char),
                ));
            }
        };
        advance(&mut i, &mut line, &mut col);
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
    fn error_unknown_word() {
        let src = "foobar";
        let err = lex(src, test_file()).unwrap_err();
        match err {
            VdlError::Lexer { location: _location, message } => {
                assert!(message.contains("unknown word: 'foobar'"));
            }
            _ => panic!("expected lexer error"),
        }
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
