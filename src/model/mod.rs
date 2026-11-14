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
    /// Imports declared by every source file in this document/project.
    pub imports: Vec<ImportDecl>,
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
    /// Optional Markdown label emitted before the body.
    pub label: Option<String>,
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
            .filter(|f| f.kind.reference_targets().is_some())
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
    /// Marks the type's heading field, rendered at the entity's depth in the composed document.
    pub heading: bool,
    /// Marks a `ref` collection for document composition.
    pub compose: bool,
    /// Emits each referenced body in place without rendering a heading.
    pub inline: bool,
    /// Marks a `ref` field whose targets may render as citation lines,
    /// per the build's cite mode.
    pub cite: bool,
    /// Allows the field to be omitted from an instance.
    pub optional: bool,
    /// Allowed entity types for `body compose ["..."]` include directives.
    pub compose_targets: Vec<String>,
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
    /// Ordered collection whose IDs may resolve to any of the listed types.
    RefsAny { targets: Vec<String> },
}

impl FieldKind {
    pub fn reference_targets(&self) -> Option<&[String]> {
        match self {
            Self::Refs { target } => Some(std::slice::from_ref(target)),
            Self::RefsAny { targets } => Some(targets),
            _ => None,
        }
    }
}

impl std::fmt::Display for FieldKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldKind::Text => write!(f, "text"),
            FieldKind::Body => write!(f, "body"),
            FieldKind::Refs { target } => write!(f, "ref {}", target),
            FieldKind::RefsAny { targets } => write!(f, "ref [{}]", targets.join(", ")),
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
    pub package: Option<String>,
    pub imports: Vec<ImportDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDecl {
    pub target: String,
    pub alias: Option<String>,
    pub wildcard: bool,
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
    /// Zero-based positions of bare IDs in the flattened reference sequence.
    pub bare_ref_indices: Vec<usize>,
    pub source_location: SourceLocation,
}

/// The value forms a field can take in an instance declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// From a quoted string.
    Text(String),
    /// From a raw triple-quoted string.
    Body(String),
    /// Raw body delimited with braces; include directives may be active here.
    BracedBody(String),
    /// From a bracketed list of quoted IDs, order preserved.
    Refs(Vec<String>),
    /// An inline composition list that also contains authored prose.
    Inline(Vec<InlineItem>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum InlineItem {
    Ref(String),
    Prose(String),
}

impl Value {
    pub(crate) fn reference_ids(&self) -> impl Iterator<Item = &str> {
        let refs = match self {
            Self::Refs(ids) => ids.as_slice(),
            _ => &[],
        };
        let inline = match self {
            Self::Inline(items) => items.as_slice(),
            _ => &[],
        };
        refs.iter().map(String::as_str).chain(inline.iter().filter_map(|item| match item {
            InlineItem::Ref(id) => Some(id.as_str()),
            InlineItem::Prose(_) => None,
        }))
    }
}

/// Return standalone `include <id>` lines outside Markdown fenced code blocks.
/// Tuple offsets are zero-based line numbers in the body and preserve the ID slice.
pub(crate) fn body_include_directives(body: &str) -> Vec<(usize, &str)> {
    let mut directives = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    for (line_index, line) in body.lines().enumerate() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let bytes = trimmed.as_bytes();
        if indent <= 3 && bytes.first().is_some_and(|b| *b == b'`' || *b == b'~') {
            let marker = bytes[0];
            let length = bytes.iter().take_while(|b| **b == marker).count();
            if length >= 3 {
                match fence {
                    None => fence = Some((marker, length)),
                    Some((open, open_len)) if marker == open && length >= open_len
                        && bytes[length..].iter().all(|b| b.is_ascii_whitespace()) => fence = None,
                    _ => {}
                }
                continue;
            }
        }
        if fence.is_none() && indent <= 3 {
            let trimmed = line.trim();
            if let Some(id) = trimmed.strip_prefix("include ") {
                let id = id.trim();
                if !id.is_empty() { directives.push((line_index, id)); }
            }
        }
    }
    directives
}

pub(crate) fn resolve_include_candidates(instance: &Instance, id: &str, doc: &SchemaDoc) -> Vec<String> {
    if let Some(exact) = id.strip_prefix('"').and_then(|id| id.strip_suffix('"')) {
        return vec![exact.to_owned()];
    }
    if id.contains('.') {
        return vec![id.to_owned()];
    }
    resolve_short_name(&instance.imports, instance.package.as_deref(), id, |id| {
        doc.instance(id).is_some()
    })
}

pub(crate) fn resolve_short_name(
    imports: &[ImportDecl],
    package: Option<&str>,
    name: &str,
    exists: impl Fn(&str) -> bool,
) -> Vec<String> {
    let explicit = imports.iter().filter(|import| {
        !import.wildcard
            && import.alias.as_deref().or_else(|| import.target.rsplit('.').next()) == Some(name)
    }).map(|import| import.target.clone()).collect::<std::collections::BTreeSet<_>>();
    if !explicit.is_empty() {
        return explicit.into_iter().collect();
    }
    if let Some(package) = package {
        let local = format!("{package}.{name}");
        if exists(&local) {
            return vec![local];
        }
    }
    let candidates = imports.iter().filter(|import| import.wildcard)
        .map(|import| format!("{}.{name}", import.target))
        .filter(|id| exists(id))
        .collect::<std::collections::BTreeSet<_>>();
    if candidates.is_empty() && package.is_none() && exists(name) {
        return vec![name.to_owned()];
    }
    candidates.into_iter().collect()
}

#[cfg(test)]
mod include_tests {
    use super::body_include_directives;

    #[test]
    fn scans_include_directives_outside_indented_and_varied_fences() {
        let body = "include first\n    include indented-code\n  ````markdown\n  include fenced\n  ```\n  include still-fenced\n  ````\ninclude last";
        let includes = body_include_directives(body);
        assert_eq!(includes.iter().map(|(_, id)| *id).collect::<Vec<_>>(), vec!["first", "last"]);
    }
}
