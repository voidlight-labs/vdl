/// A lexical token in VDL source code.
///
/// The lexer produces a stream of these tokens, which the parser consumes
/// to build the schema model.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Token {
    /// Text field kind.
    Text,

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
    /// Marks a ref field whose targets render as citation lines.
    Cite,
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
            Token::Text => Some("text"),
            Token::Schema => Some("schema"),
            Token::Type => Some("type"),
            Token::Field => Some("field"),
            Token::Ref => Some("ref"),
            Token::Compose => Some("compose"),
            Token::Inline => Some("inline"),
            Token::Cite => Some("cite"),
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
