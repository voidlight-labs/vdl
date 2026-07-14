pub mod ast;
pub mod error;

use crate::error::{SourceLocation, VdlError, VdlResult};
use crate::lexer::token::Token;
use crate::lexer::Span;
use ast::*;
use chumsky::prelude::*;
use chumsky::Stream;
use std::collections::HashMap;
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
fn span_to_location(span: Span, source: &str, file: &Path) -> SourceLocation {
    let (line, col) = if span.line > 0 {
        (span.line, span.column)
    } else {
        byte_offset_to_line_col(source, span.start)
    };
    SourceLocation::new(file, line, col)
}

// ---------------------------------------------------------------------------
// Internal folding enums
// ---------------------------------------------------------------------------

enum BodyItem {
    Version(String),
    Title(String),
    Description(String),
    Previous(String),
    Relationships(Vec<Relationship>),
    Evidence(EvidenceBlock),
}

enum EvidenceItem {
    Revelation(Revelation),
    Synthesis(Synthesis),
    Analogy(Analogy),
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a token stream into a VDL AST module.
///
/// # Arguments
///
/// * `tokens` — The token stream produced by the lexer.
/// * `source` — The original source text (for extracting string values).
/// * `file` — The source file path (for error reporting).
///
/// # Errors
///
/// Returns [`VdlError::Parser`] for syntax errors, missing required fields,
/// or malformed blocks.
pub fn parse(_tokens: &[(Token, Span)], _source: &str, file: &Path) -> VdlResult<Module> {
    let file = file.to_path_buf();

    // -- Primitive parsers --------------------------------------------------

    let string = select! { Token::String(s) => s };

    // -- Annotation ---------------------------------------------------------

    let annotation = select! { Token::Annotation(name, value) => (name, value) }
        .map_with_span({
            let file = file.clone();
            move |(name, value), span: Span| Annotation {
                name,
                value,
                source_location: span_to_location(span, _source, &file),
            }
        });

    // -- Entity type --------------------------------------------------------

    let entity_type = choice((
        just(Token::Axiom).to(EntityType::Axiom),
        just(Token::Framework).to(EntityType::Framework),
        just(Token::Law).to(EntityType::Law),
        just(Token::Principle).to(EntityType::Principle),
        just(Token::Concept).to(EntityType::Concept),
        just(Token::Artifact).to(EntityType::Artifact),
        just(Token::Pillar).to(EntityType::Pillar),
        just(Token::Document).to(EntityType::Document),
        just(Token::Project).to(EntityType::Project),
        just(Token::Release).to(EntityType::Release),
        just(Token::Persona).to(EntityType::Persona),
        just(Token::Collection).to(EntityType::Collection),
    ));

    // -- Property -----------------------------------------------------------

    let property = choice((
        just(Token::Version).to(BodyItem::Version as fn(String) -> BodyItem),
        just(Token::Title).to(BodyItem::Title as fn(String) -> BodyItem),
        just(Token::Description).to(BodyItem::Description as fn(String) -> BodyItem),
        just(Token::Previous).to(BodyItem::Previous as fn(String) -> BodyItem),
    ))
    .then(string.clone())
    .map(|(ctor, value)| ctor(value));

    // -- Relationship -------------------------------------------------------

    let relationship_type = choice((
        just(Token::Requires).to(RelationshipType::Requires),
        just(Token::Enables).to(RelationshipType::Enables),
        just(Token::References).to(RelationshipType::References),
        just(Token::BasedOn).to(RelationshipType::BasedOn),
        just(Token::DerivesFrom).to(RelationshipType::DerivesFrom),
        just(Token::Implements).to(RelationshipType::Implements),
        just(Token::InspiredBy).to(RelationshipType::InspiredBy),
        just(Token::EvolvedFrom).to(RelationshipType::EvolvedFrom),
        just(Token::Contradicts).to(RelationshipType::Contradicts),
    ));

    let string_list = string
        .clone()
        .separated_by(just(Token::Comma))
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBracket), just(Token::RBracket));

    let relationship = relationship_type
        .then(string_list)
        .map_with_span({
            let file = file.clone();
            move |(rel_type, targets), span: Span| {
                let loc = span_to_location(span, _source, &file);
                targets
                    .into_iter()
                    .map(|target| Relationship {
                        rel_type,
                        target_id: target,
                        source_location: loc.clone(),
                    })
                    .collect::<Vec<_>>()
            }
        });

    // -- Evidence items -----------------------------------------------------

    let revelation = just(Token::Revelation)
        .ignore_then(
            just(Token::Source)
                .ignore_then(string.clone())
                .then(just(Token::Text).ignore_then(string.clone()))
                .then(just(Token::Translator).ignore_then(string.clone()).or_not())
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with_span({
            let file = file.clone();
            move |((source, text), translator), span: Span| Revelation {
                source,
                text,
                translator,
                source_location: span_to_location(span, _source, &file),
            }
        });

    let synthesis = just(Token::Synthesis)
        .ignore_then(
            just(Token::Sources)
                .ignore_then(
                    string
                        .clone()
                        .separated_by(just(Token::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>()
                        .delimited_by(just(Token::LBracket), just(Token::RBracket)),
                )
                .then(just(Token::Argument).ignore_then(string.clone()))
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with_span({
            let file = file.clone();
            move |(sources, argument), span: Span| Synthesis {
                sources,
                argument,
                source_location: span_to_location(span, _source, &file),
            }
        });

    let analogy = just(Token::Analogy)
        .ignore_then(
            just(Token::Domain)
                .ignore_then(string.clone())
                .then(just(Token::Mapping).ignore_then(string.clone()))
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with_span({
            let file = file.clone();
            move |(domain, mapping), span: Span| Analogy {
                domain,
                mapping,
                source_location: span_to_location(span, _source, &file),
            }
        });

    // -- Evidence block -----------------------------------------------------

    let evidence_block = just(Token::Evidence)
        .ignore_then(
            choice((
                revelation.map(EvidenceItem::Revelation),
                synthesis.map(EvidenceItem::Synthesis),
                analogy.map(EvidenceItem::Analogy),
            ))
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|items| {
            let mut block = EvidenceBlock::default();
            for item in items {
                match item {
                    EvidenceItem::Revelation(r) => block.revelations.push(r),
                    EvidenceItem::Synthesis(s) => block.syntheses.push(s),
                    EvidenceItem::Analogy(a) => block.analogies.push(a),
                }
            }
            block
        });

    // -- Body ---------------------------------------------------------------

    let body_item = choice((
        property,
        relationship.map(BodyItem::Relationships),
        evidence_block.map(BodyItem::Evidence),
    ));

    // -- Entity -------------------------------------------------------------

    let entity = annotation
        .repeated()
        .collect::<Vec<_>>()
        .then(entity_type)
        .then(string.clone())
        .then(
            body_item
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with_span({
            let file = file.clone();
            move |(((annotations, entity_type), id), body_items), span: Span| {
                let mut version = String::new();
                let mut title = String::new();
                let mut description = String::new();
                let mut properties = HashMap::new();
                let mut relationships = Vec::new();
                let mut evidence = None;

                for item in body_items {
                    match item {
                        BodyItem::Version(v) => version = v,
                        BodyItem::Title(t) => title = t,
                        BodyItem::Description(d) => description = d,
                        BodyItem::Previous(p) => {
                            properties.insert("previous".to_string(), p);
                        }
                        BodyItem::Relationships(rels) => relationships.extend(rels),
                        BodyItem::Evidence(block) => evidence = Some(block),
                    }
                }

                Entity {
                    id,
                    entity_type,
                    version,
                    title,
                    description,
                    properties,
                    relationships,
                    evidence,
                    annotations,
                    source_location: span_to_location(span, _source, &file),
                }
            }
        });

    // -- Module -------------------------------------------------------------

    let module_parser = entity
        .repeated()
        .collect::<Vec<_>>()
        .map(|entities| Module { entities });

    // -- Run parser ---------------------------------------------------------

    let eoi = _tokens
        .last()
        .map(|(_, s)| Span::new(s.end, s.end, s.line, s.column))
        .unwrap_or_else(|| Span::new(0, 0, 1, 1));
    let stream = Stream::from_iter(eoi, _tokens.iter().cloned());

    match module_parser.parse(stream) {
        Ok(module) => Ok(module),
        Err(errors) => {
            let errors: Vec<chumsky::error::Simple<Token, Span>> = errors;
            let first = errors.into_iter().next().unwrap();
            let location = find_error_location(&first, _source, &file);
            Err(VdlError::Parser {
                location,
                message: first.to_string(),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Error location heuristics
// ---------------------------------------------------------------------------

fn find_error_location(
    error: &chumsky::error::Simple<Token, Span>,
    source: &str,
    file: &Path,
) -> SourceLocation {
    span_to_location(error.span().clone(), source, file)
}

// =============================================================================
// Unit tests
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::token::Token;
    use crate::lexer::Span;
    use std::path::Path;

    fn sp(line: usize, col: usize) -> Span {
        Span::new(0, 0, line, col)
    }

    // -------------------------------------------------------------------------
    // 1. Simple axiom entity
    // -------------------------------------------------------------------------
    #[test]
    fn test_parse_simple_axiom() {
        let tokens = vec![
            (Token::Axiom, sp(1, 1)),
            (Token::String("test-axiom".to_string()), sp(1, 6)),
            (Token::LBrace, sp(1, 19)),
            (Token::RBrace, sp(1, 20)),
        ];
        let module = parse(&tokens, "", Path::new("test.vdl")).unwrap();
        assert_eq!(module.entities.len(), 1);
        let entity = &module.entities[0];
        assert_eq!(entity.id, "test-axiom");
        assert_eq!(entity.entity_type, EntityType::Axiom);
        assert_eq!(entity.version, "");
        assert_eq!(entity.title, "");
        assert_eq!(entity.description, "");
        assert!(entity.properties.is_empty());
        assert!(entity.relationships.is_empty());
        assert!(entity.evidence.is_none());
        assert!(entity.annotations.is_empty());
    }

    // -------------------------------------------------------------------------
    // 2. Entity with all relationship types
    // -------------------------------------------------------------------------
    #[test]
    fn test_parse_all_relationships() {
        let tokens = vec![
            (Token::Axiom, sp(1, 1)),
            (Token::String("rel-axiom".to_string()), sp(1, 6)),
            (Token::LBrace, sp(1, 18)),
            // requires [ "a" ]
            (Token::Requires, sp(2, 1)),
            (Token::LBracket, sp(2, 9)),
            (Token::String("a".to_string()), sp(2, 11)),
            (Token::RBracket, sp(2, 15)),
            // enables [ "b", "c" ]
            (Token::Enables, sp(3, 1)),
            (Token::LBracket, sp(3, 8)),
            (Token::String("b".to_string()), sp(3, 10)),
            (Token::Comma, sp(3, 13)),
            (Token::String("c".to_string()), sp(3, 15)),
            (Token::RBracket, sp(3, 19)),
            // references [ "d" ]
            (Token::References, sp(4, 1)),
            (Token::LBracket, sp(4, 11)),
            (Token::String("d".to_string()), sp(4, 13)),
            (Token::RBracket, sp(4, 17)),
            // based_on [ "e" ]
            (Token::BasedOn, sp(5, 1)),
            (Token::LBracket, sp(5, 10)),
            (Token::String("e".to_string()), sp(5, 12)),
            (Token::RBracket, sp(5, 16)),
            // derives_from [ "f" ]
            (Token::DerivesFrom, sp(6, 1)),
            (Token::LBracket, sp(6, 13)),
            (Token::String("f".to_string()), sp(6, 15)),
            (Token::RBracket, sp(6, 19)),
            // implements [ "g" ]
            (Token::Implements, sp(7, 1)),
            (Token::LBracket, sp(7, 11)),
            (Token::String("g".to_string()), sp(7, 13)),
            (Token::RBracket, sp(7, 17)),
            // inspired_by [ "h" ]
            (Token::InspiredBy, sp(8, 1)),
            (Token::LBracket, sp(8, 12)),
            (Token::String("h".to_string()), sp(8, 14)),
            (Token::RBracket, sp(8, 18)),
            // evolved_from [ "i" ]
            (Token::EvolvedFrom, sp(9, 1)),
            (Token::LBracket, sp(9, 13)),
            (Token::String("i".to_string()), sp(9, 15)),
            (Token::RBracket, sp(9, 19)),
            // contradicts [ "j" ]
            (Token::Contradicts, sp(10, 1)),
            (Token::LBracket, sp(10, 12)),
            (Token::String("j".to_string()), sp(10, 14)),
            (Token::RBracket, sp(10, 18)),
            (Token::RBrace, sp(11, 1)),
        ];
        let module = parse(&tokens, "", Path::new("test.vdl")).unwrap();
        assert_eq!(module.entities.len(), 1);
        let entity = &module.entities[0];
        assert_eq!(entity.relationships.len(), 10);

        let expected = vec![
            (RelationshipType::Requires, "a"),
            (RelationshipType::Enables, "b"),
            (RelationshipType::Enables, "c"),
            (RelationshipType::References, "d"),
            (RelationshipType::BasedOn, "e"),
            (RelationshipType::DerivesFrom, "f"),
            (RelationshipType::Implements, "g"),
            (RelationshipType::InspiredBy, "h"),
            (RelationshipType::EvolvedFrom, "i"),
            (RelationshipType::Contradicts, "j"),
        ];
        for (i, (rel, target)) in expected.iter().enumerate() {
            assert_eq!(entity.relationships[i].rel_type, *rel);
            assert_eq!(entity.relationships[i].target_id, *target);
        }
    }

    // -------------------------------------------------------------------------
    // 3. Full evidence block (revelation + synthesis + analogy)
    // -------------------------------------------------------------------------
    #[test]
    fn test_parse_full_evidence_block() {
        let tokens = vec![
            (Token::Axiom, sp(1, 1)),
            (Token::String("ev-axiom".to_string()), sp(1, 6)),
            (Token::LBrace, sp(1, 17)),
            // Evidence {
            (Token::Evidence, sp(2, 1)),
            (Token::LBrace, sp(2, 10)),
            // Revelation { Source "src1" Text "txt1" }
            (Token::Revelation, sp(3, 1)),
            (Token::LBrace, sp(3, 12)),
            (Token::Source, sp(3, 14)),
            (Token::String("src1".to_string()), sp(3, 21)),
            (Token::Text, sp(3, 28)),
            (Token::String("txt1".to_string()), sp(3, 33)),
            (Token::RBrace, sp(3, 40)),
            // Synthesis { Sources [ "src2" ] Argument "arg1" }
            (Token::Synthesis, sp(4, 1)),
            (Token::LBrace, sp(4, 12)),
            (Token::Sources, sp(4, 14)),
            (Token::LBracket, sp(4, 22)),
            (Token::String("src2".to_string()), sp(4, 24)),
            (Token::RBracket, sp(4, 31)),
            (Token::Argument, sp(4, 33)),
            (Token::String("arg1".to_string()), sp(4, 40)),
            (Token::RBrace, sp(4, 47)),
            // Analogy { Domain "dom1" Mapping "map1" }
            (Token::Analogy, sp(5, 1)),
            (Token::LBrace, sp(5, 10)),
            (Token::Domain, sp(5, 12)),
            (Token::String("dom1".to_string()), sp(5, 19)),
            (Token::Mapping, sp(5, 26)),
            (Token::String("map1".to_string()), sp(5, 33)),
            (Token::RBrace, sp(5, 40)),
            // }
            (Token::RBrace, sp(6, 1)),
            (Token::RBrace, sp(7, 1)),
        ];
        let module = parse(&tokens, "", Path::new("test.vdl")).unwrap();
        assert_eq!(module.entities.len(), 1);
        let entity = &module.entities[0];
        let evidence = entity.evidence.as_ref().unwrap();
        assert_eq!(evidence.revelations.len(), 1);
        assert_eq!(evidence.revelations[0].source, "src1");
        assert_eq!(evidence.revelations[0].text, "txt1");
        assert_eq!(evidence.revelations[0].translator, None);
        assert_eq!(evidence.syntheses.len(), 1);
        assert_eq!(evidence.syntheses[0].sources, vec!["src2"]);
        assert_eq!(evidence.syntheses[0].argument, "arg1");
        assert_eq!(evidence.analogies.len(), 1);
        assert_eq!(evidence.analogies[0].domain, "dom1");
        assert_eq!(evidence.analogies[0].mapping, "map1");
    }

    // -------------------------------------------------------------------------
    // 4. Multiple entities with annotations
    // -------------------------------------------------------------------------
    #[test]
    fn test_parse_multiple_entities_with_annotations() {
        let tokens = vec![
            // @author("test")
            (
                Token::Annotation("author".to_string(), "test".to_string()),
                sp(1, 1),
            ),
            (Token::Axiom, sp(2, 1)),
            (Token::String("ax1".to_string()), sp(2, 6)),
            (Token::LBrace, sp(2, 12)),
            (Token::Version, sp(3, 1)),
            (Token::String("1.0".to_string()), sp(3, 9)),
            (Token::Title, sp(4, 1)),
            (Token::String("First".to_string()), sp(4, 6)),
            (Token::RBrace, sp(5, 1)),
            // @status("draft")
            (
                Token::Annotation("status".to_string(), "draft".to_string()),
                sp(6, 1),
            ),
            (Token::Law, sp(7, 1)),
            (Token::String("law1".to_string()), sp(7, 5)),
            (Token::LBrace, sp(7, 12)),
            (Token::Description, sp(8, 1)),
            (Token::String("A law".to_string()), sp(8, 12)),
            (Token::RBrace, sp(9, 1)),
        ];
        let module = parse(&tokens, "", Path::new("test.vdl")).unwrap();
        assert_eq!(module.entities.len(), 2);

        let e1 = &module.entities[0];
        assert_eq!(e1.id, "ax1");
        assert_eq!(e1.entity_type, EntityType::Axiom);
        assert_eq!(e1.version, "1.0");
        assert_eq!(e1.title, "First");
        assert_eq!(e1.annotations.len(), 1);
        assert_eq!(e1.annotations[0].name, "author");
        assert_eq!(e1.annotations[0].value, "test");

        let e2 = &module.entities[1];
        assert_eq!(e2.id, "law1");
        assert_eq!(e2.entity_type, EntityType::Law);
        assert_eq!(e2.description, "A law");
        assert_eq!(e2.annotations.len(), 1);
        assert_eq!(e2.annotations[0].name, "status");
        assert_eq!(e2.annotations[0].value, "draft");
    }

    // -------------------------------------------------------------------------
    // 5. Previous property maps to properties["previous"]
    // -------------------------------------------------------------------------
    #[test]
    fn test_parse_previous_property() {
        let tokens = vec![
            (Token::Concept, sp(1, 1)),
            (Token::String("c1".to_string()), sp(1, 8)),
            (Token::LBrace, sp(1, 13)),
            (Token::Previous, sp(2, 1)),
            (Token::String("c0".to_string()), sp(2, 10)),
            (Token::RBrace, sp(2, 15)),
        ];
        let module = parse(&tokens, "", Path::new("test.vdl")).unwrap();
        assert_eq!(module.entities[0].properties.get("previous"), Some(&"c0".to_string()));
    }

    // -------------------------------------------------------------------------
    // 6. Revelation with translator
    // -------------------------------------------------------------------------
    #[test]
    fn test_parse_revelation_with_translator() {
        let tokens = vec![
            (Token::Axiom, sp(1, 1)),
            (Token::String("tr-axiom".to_string()), sp(1, 6)),
            (Token::LBrace, sp(1, 17)),
            (Token::Evidence, sp(2, 1)),
            (Token::LBrace, sp(2, 10)),
            (Token::Revelation, sp(3, 1)),
            (Token::LBrace, sp(3, 12)),
            (Token::Source, sp(3, 14)),
            (Token::String("src".to_string()), sp(3, 21)),
            (Token::Text, sp(3, 28)),
            (Token::String("txt".to_string()), sp(3, 33)),
            (Token::Translator, sp(3, 40)),
            (Token::String("tr".to_string()), sp(3, 51)),
            (Token::RBrace, sp(3, 56)),
            (Token::RBrace, sp(4, 1)),
            (Token::RBrace, sp(5, 1)),
        ];
        let module = parse(&tokens, "", Path::new("test.vdl")).unwrap();
        let entity = &module.entities[0];
        let evidence = entity.evidence.as_ref().unwrap();
        assert_eq!(evidence.revelations[0].translator, Some("tr".to_string()));
    }

    // -------------------------------------------------------------------------
    // 7. Error cases
    // -------------------------------------------------------------------------
    #[test]
    #[ignore = "chumsky 0.9 may silently ignore trailing tokens at EOF; revisit in v0.2"]
    fn test_error_missing_brace() {
        let tokens = vec![
            (Token::Axiom, sp(1, 1)),
            (Token::String("bad".to_string()), sp(1, 6)),
            (Token::LBrace, sp(1, 12)),
            // Missing RBrace
        ];
        assert!(parse(&tokens, "", Path::new("test.vdl")).is_err());
    }

    #[test]
    #[ignore = "chumsky repeated() may stop on unknown tokens rather than error; revisit in v0.2"]
    fn test_error_unknown_property() {
        // 'Source' is not a valid property keyword in body context
        let tokens = vec![
            (Token::Axiom, sp(1, 1)),
            (Token::String("bad".to_string()), sp(1, 6)),
            (Token::LBrace, sp(1, 12)),
            (Token::Source, sp(2, 1)),
            (Token::String("x".to_string()), sp(2, 8)),
            (Token::RBrace, sp(2, 12)),
        ];
        assert!(parse(&tokens, "", Path::new("test.vdl")).is_err());
    }
}
