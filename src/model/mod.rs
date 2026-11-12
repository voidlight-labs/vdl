//! Data model for schema-authored VDL documents.
//!
//! A schema document declares its own entity types ([`TypeDecl`]) and then
//! instantiates them ([`Instance`]). No type, field name, or composition
//! target is known to the compiler ahead of time: everything the validator
//! and the renderer consult comes from [`SchemaDoc::schema`].
//!
//! This module is data only. Parsing lives in [`crate::parser::schema`],
//! rules in [`crate::validator::schema`], rendering in
//! [`crate::codegen::compose`].

use crate::error::SourceLocation;

/// One parsed schema document: exactly one schema block plus its instances.
///
/// A schema document is single-file by contract, which is why `schema` is not
/// an `Option`: a file without a schema block is a parse error, and a file
/// with two is a parse error as well.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaDoc {
    pub schema: Schema,
    pub entities: Vec<Instance>,
}

impl SchemaDoc {
    /// Look up a declared type by name.
    pub fn type_decl(&self, name: &str) -> Option<&TypeDecl> {
        self.schema.types.iter().find(|t| t.name == name)
    }

    /// Look up an instance by its stable ID.
    pub fn instance(&self, id: &str) -> Option<&Instance> {
        self.entities.iter().find(|e| e.id == id)
    }
}

/// The `schema "<name>" { ... }` block.
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    pub name: String,
    pub types: Vec<TypeDecl>,
    pub source_location: SourceLocation,
}

/// A user-declared entity type.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
    pub source_location: SourceLocation,
}

impl TypeDecl {
    /// Look up a declared field by name.
    pub fn field(&self, name: &str) -> Option<&FieldDecl> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// The field marked `heading`, if the type declares one.
    pub fn heading_field(&self) -> Option<&FieldDecl> {
        self.fields.iter().find(|f| f.heading)
    }

    /// The field whose kind is `body`, if the type declares one.
    pub fn body_field(&self) -> Option<&FieldDecl> {
        self.fields
            .iter()
            .find(|f| matches!(f.kind, FieldKind::Body))
    }

    /// Reference collections in declaration order.
    pub fn ref_fields(&self) -> impl Iterator<Item = &FieldDecl> {
        self.fields
            .iter()
            .filter(|f| matches!(f.kind, FieldKind::Refs { .. }))
    }

    /// Reference collections marked `compose`, in declaration order.
    pub fn composed_fields(&self) -> impl Iterator<Item = &FieldDecl> {
        self.ref_fields().filter(|f| f.compose)
    }

    /// Text fields in declaration order, used for frontmatter.
    pub fn text_fields(&self) -> impl Iterator<Item = &FieldDecl> {
        self.fields
            .iter()
            .filter(|f| matches!(f.kind, FieldKind::Text))
    }
}

/// A single field in a type declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub kind: FieldKind,
    /// Marks the type's heading field, rendered as `#` (root) or `##` (nested).
    pub heading: bool,
    /// Marks a `ref` collection for document composition.
    pub compose: bool,
    pub source_location: SourceLocation,
}

/// What a declared field holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    /// Single-line quoted string.
    Text,
    /// Multiline raw Markdown body.
    Body,
    /// Ordered collection of entity IDs whose target type is `target`.
    Refs { target: String },
}

impl std::fmt::Display for FieldKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldKind::Text => write!(f, "text"),
            FieldKind::Body => write!(f, "body"),
            FieldKind::Refs { target } => write!(f, "ref {}", target),
        }
    }
}

/// An instantiation of a schema-declared type.
#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub type_name: String,
    pub id: String,
    pub values: Vec<FieldValue>,
    pub source_location: SourceLocation,
}

impl Instance {
    /// Look up a supplied field value by name.
    ///
    /// Returns the first match; duplicate values are rejected by validation,
    /// so a valid instance has at most one value per name.
    pub fn value(&self, name: &str) -> Option<&Value> {
        self.values
            .iter()
            .find(|v| v.name == name)
            .map(|v| &v.value)
    }

    /// How many times `name` was supplied. Used by the duplicate-field rule.
    pub fn value_count(&self, name: &str) -> usize {
        self.values.iter().filter(|v| v.name == name).count()
    }
}

/// A field value written inside an instance.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldValue {
    pub name: String,
    pub value: Value,
    pub source_location: SourceLocation,
}

/// The value forms a field can take in an instance declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// From a quoted string.
    Text(String),
    /// From a raw triple-quoted string.
    Body(String),
    /// From a bracketed list of quoted IDs, order preserved.
    Refs(Vec<String>),
}
