//! Parser for schema-authored VDL documents.
//!
//! Grammar (informal):
//!
//! ```text
//! document  := annotation* schema instance*
//! schema    := "schema" STRING "{" type* "}"
//! type      := "type" STRING "{" field_decl* "}"
//! field_decl:= "field" STRING kind modifier*
//! kind      := "text" | "body" | "ref" STRING
//! modifier  := "heading" | "compose"
//! instance  := "entity" STRING STRING "{" field_value* "}"
//! field_value := "field" STRING value
//! value     := STRING | RAW_STRING | "[" STRING ("," STRING)* "]"
//! ```
//!
//! The parser stays deliberately permissive about where `heading` and
//! `compose` are allowed: `heading` on a `ref` field or `compose` on a `text`
//! field parses fine and is rejected by [`crate::validator::schema`] with a
//! located diagnostic. That keeps one authoritative place for the message.

use crate::error::{SourceLocation, VdlError, VdlResult};
use crate::lexer::token::Token;
use crate::lexer::Span;
use crate::model::{
    FieldDecl, FieldKind, FieldValue, Instance, Schema, SchemaDoc, TypeDecl, Value,
};
use chumsky::prelude::*;
use chumsky::Stream;
use std::path::Path;

/// Marker for the optional field modifiers, folded into bools after parsing.
#[derive(Debug, Clone, Copy)]
enum Modifier {
    Heading,
    Compose,
}

/// Strip the longest common leading whitespace prefix from non-blank lines.
///
/// This is raw-literal rule 5: an indented body must not turn into a Markdown
/// code block, while relative indentation and blank lines stay untouched.
pub(crate) fn dedent(raw: &str) -> String {
    let mut prefix: Option<&str> = None;

    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
        let indent = &line[..indent_len];
        prefix = Some(match prefix {
            None => indent,
            Some(current) => common_prefix(current, indent),
        });
        if prefix == Some("") {
            break;
        }
    }

    let prefix = match prefix {
        Some(p) if !p.is_empty() => p,
        _ => return raw.to_string(),
    };

    let mut out = String::with_capacity(raw.len());
    for (index, line) in raw.lines().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        if line.trim().is_empty() {
            out.push_str(line);
        } else {
            out.push_str(&line[prefix.len()..]);
        }
    }
    out
}

fn common_prefix<'a>(a: &'a str, b: &str) -> &'a str {
    let n = a
        .bytes()
        .zip(b.bytes())
        .take_while(|(x, y)| x == y)
        .count();
    &a[..n]
}

/// Parse a schema document from a token stream.
///
/// # Errors
///
/// Returns [`VdlError::Parser`] when the document does not contain exactly one
/// schema block, when the grammar does not match, or when trailing tokens
/// remain after the last instance.
pub fn parse_schema_document(
    tokens: &[(Token, Span)],
    source: &str,
    file: &Path,
) -> VdlResult<SchemaDoc> {
    let file = file.to_path_buf();

    // Single-file contract: exactly one schema block per document.
    let schema_blocks = tokens
        .iter()
        .filter(|(token, _)| matches!(token, Token::Schema))
        .count();
    if schema_blocks != 1 {
        let location = tokens
            .iter()
            .find(|(token, _)| matches!(token, Token::Schema))
            .map(|(_, span)| span.to_location(&file))
            .or_else(|| {
                tokens
                    .iter()
                    .find(|(token, _)| !matches!(token, Token::Annotation(..)))
                    .map(|(_, span)| span.to_location(&file))
            })
            .unwrap_or_else(|| SourceLocation::new(file.clone(), 1, 1));
        return Err(VdlError::Parser {
            location,
            message: format!(
                "a schema document must contain exactly one schema block, found {}",
                schema_blocks
            ),
        });
    }

    // -- Primitives ---------------------------------------------------------

    let string = select! { Token::String(s) => s };
    let raw_string = select! { Token::RawString(s) => s };

    let annotation = select! { Token::Annotation(..) => () }
        .repeated()
        .collect::<Vec<_>>();

    // -- Field declaration --------------------------------------------------

    let field_kind = choice((
        just(Token::Text).to(FieldKind::Text),
        just(Token::Body).to(FieldKind::Body),
        just(Token::Ref)
            .ignore_then(string.clone())
            .map(|target| FieldKind::Refs { target }),
    ));

    let modifier = choice((
        just(Token::Heading).to(Modifier::Heading),
        just(Token::Compose).to(Modifier::Compose),
    ));

    let field_decl = just(Token::Field)
        .ignore_then(string.clone())
        .then(field_kind)
        .then(modifier.repeated().collect::<Vec<_>>())
        .map_with_span({
            let file = file.clone();
            move |((name, kind), modifiers), span: Span| {
                let mut heading = false;
                let mut compose = false;
                for modifier in modifiers {
                    match modifier {
                        Modifier::Heading => heading = true,
                        Modifier::Compose => compose = true,
                    }
                }
                FieldDecl {
                    name,
                    kind,
                    heading,
                    compose,
                    source_location: span_to_location(span, source, &file),
                }
            }
        });

    // -- Type declaration ---------------------------------------------------

    let type_decl = just(Token::Type)
        .ignore_then(string.clone())
        .then(
            field_decl
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with_span({
            let file = file.clone();
            move |(name, fields), span: Span| TypeDecl {
                name,
                fields,
                source_location: span_to_location(span, source, &file),
            }
        });

    // -- Schema block -------------------------------------------------------

    let schema_block = just(Token::Schema)
        .ignore_then(string.clone())
        .then(
            type_decl
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with_span({
            let file = file.clone();
            move |(name, types), span: Span| Schema {
                name,
                types,
                source_location: span_to_location(span, source, &file),
            }
        });

    // -- Instances ----------------------------------------------------------

    let value = choice((
        raw_string.clone().map(|raw| Value::Body(dedent(&raw))),
        string
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map(Value::Refs),
        string.clone().map(Value::Text),
    ));

    let field_value = just(Token::Field)
        .ignore_then(string.clone())
        .then(value)
        .map_with_span({
            let file = file.clone();
            move |(name, value), span: Span| FieldValue {
                name,
                value,
                source_location: span_to_location(span, source, &file),
            }
        });

    let instance = just(Token::Entity)
        .ignore_then(string.clone())
        .then(string.clone())
        .then(
            field_value
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with_span({
            let file = file.clone();
            move |((type_name, id), values), span: Span| Instance {
                type_name,
                id,
                values,
                source_location: span_to_location(span, source, &file),
            }
        });

    // -- Document -----------------------------------------------------------

    let document = annotation
        .ignore_then(schema_block)
        .then(instance.repeated().collect::<Vec<_>>())
        .then_ignore(end())
        .map(|(schema, entities)| SchemaDoc { schema, entities });

    // -- Run ----------------------------------------------------------------

    let eoi = tokens
        .last()
        .map(|(_, s)| Span::new(s.end, s.end, s.line, s.column))
        .unwrap_or_else(|| Span::new(0, 0, 1, 1));
    let stream = Stream::from_iter(eoi, tokens.iter().cloned());

    match document.parse(stream) {
        Ok(doc) => Ok(doc),
        Err(errors) => {
            let errors: Vec<chumsky::error::Simple<Token, Span>> = errors;
            let first = errors.into_iter().next().expect("parser reported no error");
            Err(VdlError::Parser {
                location: span_to_location(first.span().clone(), source, &file),
                message: first.to_string(),
            })
        }
    }
}

/// Re-exported from `parser::mod` so both document forms share one converter.
use crate::parser::span_to_location;

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_src(src: &str) -> VdlResult<SchemaDoc> {
        let file = Path::new("test.vdl");
        let tokens = crate::lexer::lex(src, file).expect("lexing should succeed");
        parse_schema_document(&tokens, src, file)
    }

    const SCHEMA: &str = r#"
schema "runbooks" {
    type "Runbook" {
        field "label" text heading
        field "chapters" ref "Chapter" compose
        field "see_also" ref "Chapter"
    }
    type "Chapter" {
        field "label" text heading
        field "content" body
    }
}
"#;

    #[test]
    fn parses_text_field() {
        let doc = parse_src(SCHEMA).unwrap();
        let runbook = doc.type_decl("Runbook").unwrap();
        let label = runbook.field("label").unwrap();
        assert_eq!(label.kind, FieldKind::Text);
        assert!(label.heading);
        assert!(!label.compose);
    }

    #[test]
    fn parses_ref_field_with_compose() {
        let doc = parse_src(SCHEMA).unwrap();
        let chapters = doc.type_decl("Runbook").unwrap().field("chapters").unwrap();
        assert_eq!(
            chapters.kind,
            FieldKind::Refs {
                target: "Chapter".to_string()
            }
        );
        assert!(chapters.compose);
    }

    #[test]
    fn parses_body_field() {
        let doc = parse_src(SCHEMA).unwrap();
        let content = doc.type_decl("Chapter").unwrap().field("content").unwrap();
        assert_eq!(content.kind, FieldKind::Body);
        assert!(!content.heading);
    }

    #[test]
    fn parses_multiple_types_in_order() {
        let doc = parse_src(SCHEMA).unwrap();
        let names: Vec<_> = doc.schema.types.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["Runbook", "Chapter"]);
    }

    #[test]
    fn modifiers_on_wrong_kind_are_parseable() {
        // Rejected by the validator (V10/V11), not by the parser.
        let src = r#"
schema "s" {
    type "T" {
        field "x" text compose
        field "y" body heading
    }
}
"#;
        let doc = parse_src(src).unwrap();
        let t = doc.type_decl("T").unwrap();
        assert!(t.field("x").unwrap().compose);
        assert!(t.field("y").unwrap().heading);
    }

    #[test]
    fn missing_schema_block_is_parse_error() {
        let src = r#"entity "T" "a" { field "x" "1" }"#;
        let err = parse_src(src).unwrap_err();
        match err {
            VdlError::Parser { message, .. } => {
                assert!(message.contains("exactly one schema block"), "{}", message)
            }
            other => panic!("expected parser error, got {:?}", other),
        }
    }

    #[test]
    fn two_schema_blocks_is_parse_error() {
        let src = r#"
schema "a" { type "T" { field "x" text } }
schema "b" { type "U" { field "y" text } }
"#;
        let err = parse_src(src).unwrap_err();
        match err {
            VdlError::Parser { message, .. } => {
                assert!(message.contains("exactly one schema block"), "{}", message)
            }
            other => panic!("expected parser error, got {:?}", other),
        }
    }

    // -- Instances ----------------------------------------------------------

    const INSTANCE: &str = r#"
schema "runbooks" {
    type "Runbook" {
        field "label" text heading
        field "chapters" ref "Chapter" compose
    }
    type "Chapter" {
        field "label" text heading
        field "content" body
    }
}

entity "Chapter" "intro" {
    field "label" "Introduction"
    field "content" """
Prose starts here.

### Internal heading
"""
}

entity "Runbook" "core-guide" {
    field "label" "Core Guide"
    field "chapters" [ "intro" ]
}
"#;

    #[test]
    fn parses_instance_head() {
        let doc = parse_src(INSTANCE).unwrap();
        let runbook = doc.instance("core-guide").unwrap();
        assert_eq!(runbook.type_name, "Runbook");
        assert_eq!(runbook.id, "core-guide");
    }

    #[test]
    fn parses_text_value() {
        let doc = parse_src(INSTANCE).unwrap();
        assert_eq!(
            doc.instance("core-guide").unwrap().value("label"),
            Some(&Value::Text("Core Guide".to_string()))
        );
    }

    #[test]
    fn parses_ref_list_order() {
        let src = r#"
schema "s" {
    type "A" { field "items" ref "B" }
    type "B" { field "label" text heading }
}
entity "A" "a" { field "items" [ "b", "a" ] }
"#;
        let doc = parse_src(src).unwrap();
        assert_eq!(
            doc.instance("a").unwrap().value("items"),
            Some(&Value::Refs(vec!["b".to_string(), "a".to_string()]))
        );
    }

    #[test]
    fn parses_empty_ref_list() {
        let src = r#"
schema "s" {
    type "A" { field "items" ref "B" }
    type "B" { field "label" text heading }
}
entity "A" "a" { field "items" [ ] }
"#;
        let doc = parse_src(src).unwrap();
        assert_eq!(
            doc.instance("a").unwrap().value("items"),
            Some(&Value::Refs(vec![]))
        );
    }

    #[test]
    fn body_dedent_strips_common_indent() {
        let raw = "    Line one\n\n        Nested\n    Line two";
        assert_eq!(dedent(raw), "Line one\n\n    Nested\nLine two");
    }

    #[test]
    fn body_preserves_blank_lines() {
        let raw = "    A\n\n\n    B";
        assert_eq!(dedent(raw), "A\n\n\nB");
    }

    #[test]
    fn body_fidelity_after_documented_transform() {
        let src = "schema \"s\" {\n    type \"T\" { field \"content\" body }\n}\n\nentity \"T\" \"a\" {\n    field \"content\" \"\"\"\r\n    # Title\r\n\r\n    Paragraph with  two  spaces.\r\n    \"\"\"\n}\n";
        let doc = parse_src(src).unwrap();
        assert_eq!(
            doc.instance("a").unwrap().value("content"),
            Some(&Value::Body(
                "# Title\n\nParagraph with  two  spaces.".to_string()
            ))
        );
    }
}
