/// A lexical token in VDL source code.
///
/// The lexer produces a stream of these tokens, which the parser consumes
/// to build the AST. Every keyword in VDL v0.1 is represented explicitly;
/// no bare identifiers exist outside of string literals.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Token {
    // --- Entity type keywords ---
    Axiom,
    Framework,
    Law,
    Principle,
    Concept,
    Artifact,
    Pillar,
    Document,
    Project,
    Release,
    Persona,
    Collection,

    // --- Property keywords ---
    Version,
    Title,
    Description,
    Previous,

    // --- Relationship keywords ---
    Requires,
    Enables,
    References,
    BasedOn,
    DerivesFrom,
    Implements,
    InspiredBy,
    EvolvedFrom,
    Contradicts,

    // --- Evidence keywords ---
    Evidence,
    Revelation,
    Synthesis,
    Analogy,
    // Evidence property keywords
    Source,
    Text,
    Translator,
    Sources,
    Argument,
    Domain,
    Mapping,

    // --- Schema document keywords ---
    /// Starts a `schema "<name>" { ... }` document block.
    Schema,
    /// Declares a type inside a schema block.
    Type,
    /// Declares a field inside a type, or supplies a value inside an instance.
    Field,
    /// Declares a reference collection field.
    Ref,
    /// Marks a reference collection for document composition.
    Compose,
    /// Declares a multiline Markdown body field.
    Body,
    /// Marks a text field as the type's heading.
    Heading,
    /// Starts an `entity "<Type>" "<id>" { ... }` instance declaration.
    Entity,

    // --- Literals ---
    /// A double-quoted string literal. Supports multiline and `\"` / `\\` escapes.
    String(String),
    /// A raw triple-quoted body literal. No escape processing; line endings are
    /// normalized to LF and the surrounding blank lines are trimmed.
    RawString(String),

    // --- Annotations ---
    /// An annotation token: `@name("value")`.
    /// The first string is the annotation name, the second is its value.
    Annotation(String, String),

    // --- Delimiters ---
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Axiom => write!(f, "axiom"),
            Token::Framework => write!(f, "framework"),
            Token::Law => write!(f, "law"),
            Token::Principle => write!(f, "principle"),
            Token::Concept => write!(f, "concept"),
            Token::Artifact => write!(f, "artifact"),
            Token::Pillar => write!(f, "pillar"),
            Token::Document => write!(f, "document"),
            Token::Project => write!(f, "project"),
            Token::Release => write!(f, "release"),
            Token::Persona => write!(f, "persona"),
            Token::Collection => write!(f, "collection"),
            Token::Version => write!(f, "version"),
            Token::Title => write!(f, "title"),
            Token::Description => write!(f, "description"),
            Token::Previous => write!(f, "previous"),
            Token::Requires => write!(f, "requires"),
            Token::Enables => write!(f, "enables"),
            Token::References => write!(f, "references"),
            Token::BasedOn => write!(f, "based_on"),
            Token::DerivesFrom => write!(f, "derives_from"),
            Token::Implements => write!(f, "implements"),
            Token::InspiredBy => write!(f, "inspired_by"),
            Token::EvolvedFrom => write!(f, "evolved_from"),
            Token::Contradicts => write!(f, "contradicts"),
            Token::Evidence => write!(f, "evidence"),
            Token::Revelation => write!(f, "revelation"),
            Token::Synthesis => write!(f, "synthesis"),
            Token::Analogy => write!(f, "analogy"),
            Token::Source => write!(f, "source"),
            Token::Text => write!(f, "text"),
            Token::Translator => write!(f, "translator"),
            Token::Sources => write!(f, "sources"),
            Token::Argument => write!(f, "argument"),
            Token::Domain => write!(f, "domain"),
            Token::Mapping => write!(f, "mapping"),
            Token::Schema => write!(f, "schema"),
            Token::Type => write!(f, "type"),
            Token::Field => write!(f, "field"),
            Token::Ref => write!(f, "ref"),
            Token::Compose => write!(f, "compose"),
            Token::Body => write!(f, "body"),
            Token::Heading => write!(f, "heading"),
            Token::Entity => write!(f, "entity"),
            Token::String(s) => write!(f, "{:?}", s),
            Token::RawString(s) => write!(f, "{:?}", s),
            Token::Annotation(name, value) => write!(f, "@{}({:?})", name, value),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::Comma => write!(f, ","),
        }
    }
}
