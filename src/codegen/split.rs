use crate::model::{body_include_directives, resolve_include_candidates, InlineItem, SchemaDoc, Value};
use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) type LinkTargets = HashMap<String, (String, String)>;

/// Keep output paths shallow and deterministic: one package directory and
/// the complete stable entity ID as the filename.
pub(crate) fn output_path(id: &str) -> PathBuf {
    let package = id.split('.').next().unwrap_or(id);
    PathBuf::from(package).join(format!("{}.md", id))
}

pub(crate) fn valid_entity_id_for_filename(id: &str) -> bool {
    !id.is_empty()
        && !is_windows_device_name(id.split('.').next().unwrap_or(id))
        && id.split('.').all(|segment| {
            !segment.is_empty()
                && segment.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        })
}

fn is_windows_device_name(name: &str) -> bool {
    let name = name.trim_end_matches([' ', '.']).to_ascii_uppercase();
    matches!(name.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            name.strip_prefix(prefix).is_some_and(|suffix| matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
        })
}

pub(crate) fn filename_exceeds_windows_component_limit(id: &str) -> bool {
    id.encode_utf16().count() + 3 > 255
}

pub(crate) fn render_entity(
    doc: &SchemaDoc,
    id: &str,
    links: &LinkTargets,
    contents: &[(String, Vec<String>)],
    navigation: &[(String, Option<String>, Option<String>)],
) -> Option<String> {
    let entity = doc.instance(id)?;
    let entity_type = doc.type_decl(&entity.type_name)?;
    let title = entity_type
        .heading_field()
        .and_then(|field| text_value(entity.value(&field.name)))
        .unwrap_or(id);
    let mut out = format!(
        "---\ntitle: {}\nid: {}\n",
        crate::codegen::compose::yaml_double_quoted(title),
        crate::codegen::compose::yaml_double_quoted(id)
    );
    for name in ["version", "status"] {
        if let Some(value) = entity_type
            .text_fields()
            .find(|field| field.name == name)
            .and_then(|field| text_value(entity.value(&field.name)))
        {
            out.push_str(&format!(
                "{}: {}\n",
                name,
                crate::codegen::compose::yaml_double_quoted(value)
            ));
        }
    }
    out.push_str(&format!("---\n\n# {}\n\n", title));

    let mut bodies = Vec::new();
    for field in &entity_type.fields {
        match entity.value(&field.name) {
            Some(Value::Text(value)) if !field.heading => {
                out.push_str(&format!("**{}:** {}\n\n", field.name, value));
            }
            Some(Value::Body(value)) => bodies.push((field, value, false)),
            Some(Value::BracedBody(value)) => bodies.push((field, value, true)),
            Some(Value::Refs(ids)) => out.push_str(&format!(
                "**{}:** {}\n\n",
                field.name,
                ids.iter()
                    .map(|target| link(target, links))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Some(Value::Inline(items)) => out.push_str(&format!(
                "**{}:** {}\n\n",
                field.name,
                render_inline(items, links)
            )),
            _ => {}
        }
    }
    for (field, body, braced) in bodies {
        out.push_str(&render_body(doc, entity, field, body, braced, links));
        out.push_str("\n\n");
    }
    for (label, children) in contents {
        if children.is_empty() {
            continue;
        }
        out.push_str(&format!("## {}\n\n", label));
        for (index, child) in children.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", index + 1, link(child, links)));
        }
        out.push('\n');
    }
    for (container, previous, next) in navigation {
        out.push_str(&format!(
            "**Navigation {}:** {}{}\n",
            link(container, links),
            previous
                .as_deref()
                .map(|id| format!("← {}", link(id, links)))
                .unwrap_or_default(),
            next.as_deref()
                .map(|id| format!("{}{} →", if previous.is_some() { " | " } else { "" }, link(id, links)))
                .unwrap_or_default()
        ));
    }
    Some(out)
}

fn render_body(
    doc: &SchemaDoc,
    entity: &crate::model::Instance,
    field: &crate::model::FieldDecl,
    body: &str,
    braced: bool,
    links: &LinkTargets,
) -> String {
    if !braced || !matches!(field.kind, crate::model::FieldKind::Body) || field.compose_targets.is_empty() {
        return body.to_owned();
    }
    let directives = body_include_directives(body);
    body.lines()
        .enumerate()
        .map(|(index, line)| match directives.iter().find(|(line_index, _)| *line_index == index) {
            Some((_, name)) => match resolve_include_candidates(entity, name, doc).as_slice() {
                [target] => link(target, links),
                _ => String::new(),
            },
            None => line.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_inline(items: &[InlineItem], links: &LinkTargets) -> String {
    items
        .iter()
        .map(|item| match item {
            InlineItem::Ref(id) => link(id, links),
            InlineItem::Prose(text) => text.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn text_value(value: Option<&Value>) -> Option<&str> {
    match value {
        Some(Value::Text(text)) if !text.trim().is_empty() => Some(text),
        _ => None,
    }
}

fn link(id: &str, links: &LinkTargets) -> String {
    links
        .get(id)
        .map(|(path, title)| format!("[{}]({})", markdown_label(title), path))
        .unwrap_or_else(|| markdown_label(id))
}

fn markdown_label(value: &str) -> String {
    value.replace('\\', "\\\\").replace('[', "\\[").replace(']', "\\]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_uses_full_id_in_a_shallow_package_folder() {
        assert_eq!(output_path("soul.axiom-zero.wujud-mumkin"), PathBuf::from("soul/soul.axiom-zero.wujud-mumkin.md"));
        assert_ne!(output_path("soul.one"), output_path("other.one"));
    }

    #[test]
    fn output_filename_has_no_hash_suffix_and_short_path() {
        let path = output_path("soul.divine-alignment.wujud-mumkin");
        assert_eq!(path.components().count(), 2);
        assert_eq!(path.file_name().unwrap(), "soul.divine-alignment.wujud-mumkin.md");
        assert!(path.to_string_lossy().len() < 260);
        assert!(!path.to_string_lossy().contains("-b393607e99cef575"));
    }

    #[test]
    fn filename_ids_must_match_the_published_id_pattern() {
        assert!(valid_entity_id_for_filename("soul.axiom_zero"));
        assert!(valid_entity_id_for_filename("demo.include-root"));
        assert!(valid_entity_id_for_filename("quran.30.30"));
        assert!(!valid_entity_id_for_filename("soul..empty"));
        assert!(!valid_entity_id_for_filename("soul/unsafe"));
        assert!(!valid_entity_id_for_filename("con.foo"));
    }

    #[test]
    fn windows_filename_component_limit_includes_extension() {
        assert!(!filename_exceeds_windows_component_limit(&"a".repeat(252)));
        assert!(filename_exceeds_windows_component_limit(&"a".repeat(253)));
    }

    #[test]
    fn markdown_link_escapes_label_and_keeps_relative_target() {
        let mut links = HashMap::new();
        links.insert("a[b]".to_owned(), ("../soul/a[b].md".to_owned(), "A [title]".to_owned()));
        assert_eq!(link("a[b]", &links), "[A \\[title\\]](../soul/a[b].md)");
    }
}
