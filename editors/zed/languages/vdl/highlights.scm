[
  "package" "import" "as" "schema" "type" "field" "entity" "label"
  "text" "body" "ref" "in" "heading" "compose" "inline" "optional" "evidence"
] @keyword

(package_declaration namespace: (name) @type)
(import_declaration path: (name) @type)
(import_declaration alias: (name) @constant)
(type_declaration name: (string) @type)
(entity_declaration type: (_) @type)
(field_kind target: (string) @type)
(field_declaration target: (string) @type)
(field_declaration name: (string) @property)
(field_value name: (_) @property)
(braced_field_name) @property
(entity_declaration id: (entity_id) @constant)
(reference_list (entity_id) @constant)
(legacy_evidence_item ["revelation" "synthesis" "analogy"] @keyword)
(annotation) @attribute

[
  (string)
  (raw_open)
] @string

[
  (line_comment)
  (block_comment)
] @comment

["{" "}" "[" "]" "(" ")"] @punctuation.bracket
[","] @punctuation.delimiter

[(body_open) (body_close)] @punctuation.bracket

(import_declaration "*" @operator)
