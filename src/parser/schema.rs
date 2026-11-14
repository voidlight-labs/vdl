//! Parser for schema-authored VDL documents.
//!
//! Grammar (informal):
//!
//! ```text
//! document  := annotation* package? schema? instance*
//! package   := "package" QUALIFIED_NAME
//! schema    := "schema" STRING "{" type* "}"
//! type      := "type" STRING ("label" STRING)? "{" field_decl* "}"
//! field_decl:= "field" STRING kind modifier*
//! kind      := "text" | "body" | "ref" (STRING | "[" STRING ("," STRING)* "]")
//! modifier  := "heading" | "compose" | "inline" | "optional"
//! instance  := ("entity" STRING | NAME) id "{" field_value* "}"
//! field_value := ("field" STRING | NAME) value
//! value     := STRING | RAW_STRING | "[" (item ("," item)*)? "]"
//! item      := id | "prose" RAW_STRING
//! id        := STRING | QUALIFIED_NAME
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
    FieldDecl, FieldKind, FieldValue, ImportDecl, InlineItem, Instance, Schema, SchemaDoc, TypeDecl, Value,
};
use chumsky::prelude::*;
use chumsky::Stream;
use std::path::Path;

/// Marker for the optional field modifiers, folded into bools after parsing.
#[derive(Debug, Clone, Copy)]
enum Modifier {
    Heading,
    Compose,
    Inline,
    Cite,
    Optional,
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

/// One file's contribution to a schema project: an optional schema block plus
/// the instances declared in that file.
///
/// The single-file API ([`parse_schema_document`]) requires exactly one schema
/// block. Project loading uses this fragment form so instance-only files are
/// legal and the schema may live in any file.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaFragment {
    pub schema: Option<Schema>,
    pub entities: Vec<Instance>,
    pub imports: Vec<ImportDecl>,
}

fn count_schema_blocks(tokens: &[(Token, Span)]) -> usize {
    schema_block_spans(tokens).count()
}

fn schema_block_spans(tokens: &[(Token, Span)]) -> impl Iterator<Item = &Span> {
    let mut depth = 0usize;
    tokens.iter().enumerate().filter_map(move |(index, (token, span))| {
        match token {
            Token::Schema if depth == 0
                && (index == 0 || matches!(tokens.get(index + 1), Some((Token::String(_), _)))) =>
            {
                Some(span)
            }
            Token::LBrace => {
                depth += 1;
                None
            }
            Token::RBrace => {
                depth = depth.saturating_sub(1);
                None
            }
            _ => None,
        }
    })
}

fn skip_annotations(tokens: &[(Token, Span)]) -> &[(Token, Span)] {
    let first = tokens
        .iter()
        .position(|(token, _)| !matches!(token, Token::Annotation(..)))
        .unwrap_or(tokens.len());
    &tokens[first..]
}

pub(crate) fn first_declaration(tokens: &[(Token, Span)]) -> Option<&Token> {
    let tokens = skip_annotations(tokens);
    let mut i = if matches!(tokens.first().map(|(t, _)| t), Some(Token::Package)) { 2 } else { 0 };
    while matches!(tokens.get(i).map(|(t, _)| t), Some(Token::Import)) {
        i += 2;
        if matches!(tokens.get(i).map(|(t, _)| t), Some(Token::Star)) { i += 1; }
        if matches!(tokens.get(i).map(|(t, _)| t), Some(Token::As)) { i += 2; }
    }
    tokens.get(i).map(|(token, _)| token)
}

pub(crate) fn declares_schema(tokens: &[(Token, Span)]) -> bool {
    first_declaration(tokens).is_some_and(|t| matches!(t, Token::Schema))
}

fn qualified_name(token: &Token) -> Option<&str> {
    let word = token.word()?;
    word.split('.')
        .all(|segment| {
            !segment.is_empty()
                && !segment.starts_with('-')
                && !segment.ends_with('-')
                && segment.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        })
        .then_some(word)
}

fn split_package<'a>(
    tokens: &'a [(Token, Span)],
    file: &Path,
) -> VdlResult<(Option<&'a str>, &'a [(Token, Span)])> {
    let tokens = skip_annotations(tokens);
    let [(Token::Package, keyword), rest @ ..] = tokens else {
        return Ok((None, tokens));
    };
    if let [(name, _), declarations @ ..] = rest {
        if let Some(name) = qualified_name(name) {
            return Ok((Some(name), declarations));
        }
    }
    Err(VdlError::Parser {
        location: rest
            .first()
            .map_or(keyword, |(_, span)| span)
            .to_location(file),
        message: "expected a package name such as soul.alignment".into(),
    })
}

fn split_imports<'a>(tokens: &'a [(Token, Span)], file: &Path) -> VdlResult<(Vec<ImportDecl>, &'a [(Token, Span)])> {
    let mut imports = Vec::new();
    let mut offset = 0usize;
    while matches!(tokens.get(offset).map(|(t, _)| t), Some(Token::Import)) {
        let (_, span) = &tokens[offset];
        let Some((Token::Identifier(path), _)) = tokens.get(offset + 1) else {
            return Err(VdlError::Parser { location: span.to_location(file), message: "expected import path".into() });
        };
        let (target, wildcard, consumed) = if path.ends_with('.') && matches!(tokens.get(offset + 2).map(|(t, _)| t), Some(Token::Star)) {
            (path.trim_end_matches('.').to_owned(), true, 3)
        } else { (path.clone(), false, 2) };
        let alias = if matches!(tokens.get(offset + consumed).map(|(t, _)| t), Some(Token::As)) {
            match tokens.get(offset + consumed + 1) {
                Some((name, _)) => name.word().map(str::to_owned),
                None => None,
            }
        } else { None };
        let alias_len = usize::from(alias.is_some()) * 2;
        imports.push(ImportDecl { target, alias, wildcard, source_location: span.to_location(file) });
        offset += consumed + alias_len;
    }
    Ok((imports, &tokens[offset..]))
}

/// Identifies schema syntax without a project schema. A bare type that also
/// starts a legacy declaration is resolved by the project loader.
pub(crate) fn starts_schema_document(tokens: &[(Token, Span)]) -> bool {
    first_declaration(tokens).is_some_and(|token| {
        token.word().is_some()
            && (super::legacy_entity_type(token).is_none()
                || tokens.iter().any(|(token, _)| {
                    matches!(token, Token::Identifier(_) | Token::RawString(_) | Token::Field)
                }))
    })
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
    let (_, declarations) = split_package(tokens, &file)?;
    let schema_blocks = count_schema_blocks(declarations);
    if schema_blocks != 1 {
        let location = schema_block_spans(declarations)
            .next()
            .map(|span| span.to_location(&file))
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

    let fragment = parse_schema_fragment(tokens, source, &file)?;
    let schema = fragment
        .schema
        .expect("a document with one schema block parses one schema");
    let mut doc = SchemaDoc {
        schema,
        entities: fragment.entities,
        imports: fragment.imports,
    };
    crate::project::resolve_import_references(&mut doc)?;
    Ok(doc)
}

/// Parse one file that may declare a schema block and/or instances.
///
/// Zero schema blocks is legal here: an instance-only file is part of a
/// project whose schema lives elsewhere. Two in one file is still an error.
pub fn parse_schema_fragment(
    tokens: &[(Token, Span)],
    source: &str,
    file: &Path,
) -> VdlResult<SchemaFragment> {
    let file = file.to_path_buf();

    let (package, tokens) = split_package(tokens, &file)?;
    let (imports, tokens) = split_imports(tokens, &file)?;
    let schema_blocks = count_schema_blocks(tokens);
    if schema_blocks > 1 {
        let location = schema_block_spans(tokens)
            .next()
            .map(|span| span.to_location(&file))
            .unwrap_or_else(|| SourceLocation::new(file.clone(), 1, 1));
        return Err(VdlError::Parser {
            location,
            message: format!(
                "a document may contain at most one schema block, found {}",
                schema_blocks
            ),
        });
    }

    // -- Primitives ---------------------------------------------------------

    let string = select! { Token::String(s) => s };
    let raw_string = select! { Token::RawString(s) => s };
    let braced_string = select! { Token::BracedBody(s) => s };

    // -- Field declaration --------------------------------------------------

    let field_kind = choice((
        just(Token::Text).to(FieldKind::Text),
        just(Token::Body).to(FieldKind::Body),
        just(Token::Ref).ignore_then(
            string.clone().map(|target| FieldKind::Refs { target }).or(
                string
                    .clone()
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .allow_trailing()
                    .delimited_by(just(Token::LBracket), just(Token::RBracket))
                    .map(|targets| FieldKind::RefsAny { targets }),
            ),
        ),
    ));

    let modifier = choice((
        just(Token::Heading).to(Modifier::Heading),
        just(Token::Compose).to(Modifier::Compose),
        just(Token::Inline).to(Modifier::Inline),
        just(Token::Cite).to(Modifier::Cite),
        just(Token::Optional).to(Modifier::Optional),
    ));

    let field_decl = just(Token::Field)
        .ignore_then(string.clone())
        .then(field_kind)
        .then(modifier.clone().repeated().collect::<Vec<_>>())
        .then(string.clone().separated_by(just(Token::Comma)).at_least(1).allow_trailing().delimited_by(just(Token::LBracket), just(Token::RBracket)).or_not())
        .then(modifier.repeated().collect::<Vec<_>>())
        .map_with_span({
            let file = file.clone();
            move |((((name, kind), mut modifiers), compose_targets), trailing_modifiers), span: Span| {
                modifiers.extend(trailing_modifiers);
                let mut heading = false;
                let mut compose = false;
                let mut inline = false;
                let mut cite = false;
                let mut optional = false;
                for modifier in modifiers {
                    match modifier {
                        Modifier::Heading => heading = true,
                        Modifier::Compose => compose = true,
                        Modifier::Inline => inline = true,
                        Modifier::Cite => cite = true,
                        Modifier::Optional => optional = true,
                    }
                }
                FieldDecl {
                    name,
                    kind,
                    heading,
                    compose: compose || inline,
                    inline,
                    cite,
                    optional,
                    compose_targets: compose_targets.unwrap_or_default(),
                    source_location: span_to_location(span, source, &file),
                }
            }
        });

    // -- Type declaration ---------------------------------------------------

    let type_decl = just(Token::Type)
        .ignore_then(string.clone())
        .then(just(Token::Label).ignore_then(string.clone()).or_not())
        .then(
            field_decl
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with_span({
            let file = file.clone();
            move |((name, label), fields), span: Span| TypeDecl {
                name,
                fields,
                label,
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

    let id = string.clone().or(filter_map(move |span, token: Token| {
        let name = qualified_name(&token)
            .ok_or_else(|| Simple::custom(span, "expected an entity name or quoted ID"))?;
        Ok(match package {
            Some(package) if !name.contains('.') => format!("{}.{}", package, name),
            _ => name.to_owned(),
        })
    }));

    let ref_id = string.clone().map(|name| (name, false)).or(filter_map(move |span, token: Token| {
        let name = qualified_name(&token)
            .ok_or_else(|| Simple::custom(span, "expected an entity name or quoted ID"))?;
        Ok((name.to_owned(), !name.contains('.')))
    }));

    let value = choice((
        braced_string.clone().map(|raw| (Value::BracedBody(dedent(&raw)), Vec::new())),
        raw_string.clone().map(|raw| (Value::Body(dedent(&raw)), Vec::new())),
        just(Token::Prose)
            .ignore_then(raw_string.clone().or(braced_string.clone()))
            .map(|raw| (InlineItem::Prose(dedent(&raw)), false))
            .or(ref_id.map(|(id, bare)| (InlineItem::Ref(id), bare)))
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map(|items| {
                let mut bare_refs = Vec::new();
                let mut ref_index = 0;
                for (item, bare) in &items {
                    if matches!(item, InlineItem::Ref(_)) {
                        if *bare { bare_refs.push(ref_index); }
                        ref_index += 1;
                    }
                }
                if items.iter().any(|(item, _)| matches!(item, InlineItem::Prose(_))) {
                    (Value::Inline(items.into_iter().map(|(item, _)| item).collect()), bare_refs)
                } else {
                    (Value::Refs(items.into_iter().filter_map(|(item, _)| match item {
                        InlineItem::Ref(id) => Some(id), InlineItem::Prose(_) => None,
                    }).collect()), bare_refs)
                }
            }),
        string.clone().map(|text| (Value::Text(text), Vec::new())),
    ));

    let bare_name = filter_map(|span, token: Token| {
        token
            .word()
            .filter(|word| {
                word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                    && word.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
            .map(str::to_owned)
            .ok_or_else(|| Simple::custom(span, "expected a bare name"))
    });
    let field_value = just(Token::Field)
        .ignore_then(string.clone())
        .then(value.clone())
        .or(bare_name.clone().then(value))
        .map_with_span({
            let file = file.clone();
            move |(name, (value, bare_ref_indices)), span: Span| FieldValue {
                name,
                value,
                bare_ref_indices,
                source_location: span_to_location(span, source, &file),
            }
        });

    let instance_imports = imports.clone();
    let instance = just(Token::Entity)
        .ignore_then(string.clone())
        .or(none_of([Token::Schema, Token::Entity, Token::Package])
            .rewind()
            .ignore_then(bare_name))
        .then(id)
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
                package: package.map(str::to_owned),
                imports: instance_imports.clone(),
            }
        });

    // -- Document -----------------------------------------------------------

    let document = schema_block.or_not()
        .then(instance.repeated().collect::<Vec<_>>())
        .then_ignore(end())
        .map(|(schema, entities)| SchemaFragment { schema, entities, imports: imports.clone() });

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
    fn fragment_accepts_instance_only_file() {
        let src = r#"entity "T" "a" { field "x" "1" }"#;
        let file = Path::new("only-entities.vdl");
        let tokens = crate::lexer::lex(src, file).expect("lexing should succeed");
        let fragment = parse_schema_fragment(&tokens, src, file).expect("fragment parses");
        assert!(fragment.schema.is_none());
        assert_eq!(fragment.entities.len(), 1);
        assert_eq!(fragment.entities[0].id, "a");
    }

    #[test]
    fn compact_names_work_with_author_defined_types_and_keyword_fields() {
        let src = r#"
schema "test" {
    type "Runbook" { field "summary" text field "schema" text }
}
Runbook "deploy" { summary "Deploy guide" schema "v1" }
"#;
        let file = Path::new("runbook.vdl");
        let tokens = crate::lexer::lex(src, file).unwrap();
        let doc = parse_schema_document(&tokens, src, file).unwrap();
        crate::validator::schema::validate_schema(&doc).unwrap();
        let entity = doc.instance("deploy").unwrap();
        assert_eq!(entity.type_name, "Runbook");
        assert_eq!(entity.value("schema"), Some(&Value::Text("v1".into())));
    }

    #[test]
    fn fragment_rejects_two_schema_blocks_in_one_file() {
        let src = r#"
schema "a" { type "T" { field "x" text } }
schema "b" { type "U" { field "y" text } }
"#;
        let file = Path::new("two.vdl");
        let tokens = crate::lexer::lex(src, file).expect("lexing should succeed");
        match parse_schema_fragment(&tokens, src, file) {
            Err(VdlError::Parser { message, .. }) => {
                assert!(message.contains("at most one schema block"), "{}", message)
            }
            other => panic!("expected parser error, got {:?}", other),
        }
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
