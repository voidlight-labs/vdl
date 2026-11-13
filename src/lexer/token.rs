/// A lexical token in VDL source code.
///
/// The lexer produces a stream of these tokens, which the parser consumes
/// to build the AST. Keywords retain their own tokens for the legacy grammar.
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
    /// Composes referenced bodies without rendering headings.
    Inline,
    /// Starts a literal prose item in an inline list.
    Prose,
    /// Declares a label for a type's rendered body.
    Label,
    /// Declares a multiline Markdown body field.
    Body,
    /// Marks a text field as the type's heading.
    Heading,
    /// Allows a schema field to be omitted.
    Optional,
    /// Starts an `entity "<Type>" "<id>" { ... }` instance declaration.
    Entity,
    /// Declares the logical namespace of a schema source file.
    Package,
    Import,
    As,
    Star,

    /// A bare name that is not a language keyword.
    Identifier(String),

    // --- Literals ---
    /// A double-quoted string literal. Supports multiline and `\"` / `\\` escapes.
    String(String),
    /// A raw triple-quoted body literal. No escape processing; line endings are
    /// normalized to LF and the surrounding blank lines are trimmed.
    RawString(String),
    /// Raw Markdown read from a braced field body (distinct for include parsing).
    BracedBody(String),

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

impl Token {
    /// The original spelling of a bare name, including keyword spellings.
    pub(crate) fn word(&self) -> Option<&str> {
        match self {
            Token::Axiom => Some("axiom"),
            Token::Framework => Some("framework"),
            Token::Law => Some("law"),
            Token::Principle => Some("principle"),
            Token::Concept => Some("concept"),
            Token::Artifact => Some("artifact"),
            Token::Pillar => Some("pillar"),
            Token::Document => Some("document"),
            Token::Project => Some("project"),
            Token::Release => Some("release"),
            Token::Persona => Some("persona"),
            Token::Collection => Some("collection"),
            Token::Version => Some("version"),
            Token::Title => Some("title"),
            Token::Description => Some("description"),
            Token::Previous => Some("previous"),
            Token::Requires => Some("requires"),
            Token::Enables => Some("enables"),
            Token::References => Some("references"),
            Token::BasedOn => Some("based_on"),
            Token::DerivesFrom => Some("derives_from"),
            Token::Implements => Some("implements"),
            Token::InspiredBy => Some("inspired_by"),
            Token::EvolvedFrom => Some("evolved_from"),
            Token::Contradicts => Some("contradicts"),
            Token::Evidence => Some("evidence"),
            Token::Revelation => Some("revelation"),
            Token::Synthesis => Some("synthesis"),
            Token::Analogy => Some("analogy"),
            Token::Source => Some("source"),
            Token::Text => Some("text"),
            Token::Translator => Some("translator"),
            Token::Sources => Some("sources"),
            Token::Argument => Some("argument"),
            Token::Domain => Some("domain"),
            Token::Mapping => Some("mapping"),
            Token::Schema => Some("schema"),
            Token::Type => Some("type"),
            Token::Field => Some("field"),
            Token::Ref => Some("ref"),
            Token::Compose => Some("compose"),
            Token::Inline => Some("inline"),
            Token::Prose => Some("prose"),
            Token::Label => Some("label"),
            Token::Body => Some("body"),
            Token::Heading => Some("heading"),
            Token::Optional => Some("optional"),
            Token::Entity => Some("entity"),
            Token::Package => Some("package"),
            Token::Import => Some("import"),
            Token::As => Some("as"),
            Token::Star => Some("*"),
            Token::Identifier(name) => Some(name),
            _ => None,
        }
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(word) = self.word() {
            return f.write_str(word);
        }
        match self {
            Token::String(s) => write!(f, "{:?}", s),
            Token::RawString(s) => write!(f, "{:?}", s),
            Token::BracedBody(s) => write!(f, "{:?}", s),
            Token::Annotation(name, value) => write!(f, "@{}({:?})", name, value),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::Comma => write!(f, ","),
            Token::Star => write!(f, "*"),
            _ => unreachable!("bare names are handled above"),
        }
    }
}
