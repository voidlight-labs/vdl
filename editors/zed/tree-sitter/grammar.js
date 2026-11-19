module.exports = grammar({
  name: "vdl",

  extras: $ => [/[ \t\r\n]/, $.line_comment, $.block_comment],
  externals: $ => [$.braced_field_name, $.body_open, $.raw_open, $.markdown_line, $.body_close],
  word: $ => $.name,
  conflicts: $ => [[$.entity_declaration], [$.field_declaration]],

  rules: {
    source_file: $ => seq(
      optional(seq(repeat($.annotation), optional($.package_declaration), repeat($.import_declaration), optional($.schema_declaration))),
      repeat($.entity_declaration),
    ),

    package_declaration: $ => seq("package", field("namespace", $.name)),
    import_declaration: $ => seq("import", field("path", $.name), optional("*"), optional(seq("as", field("alias", $.name)))),

    schema_declaration: $ => seq(
      "schema", field("name", $.string), "{", repeat($.type_declaration), "}",
    ),

    type_declaration: $ => seq(
      "type", field("name", $.string), optional(seq("label", $.string)),
      "{", repeat($.field_declaration), "}",
    ),

    field_declaration: $ => seq(
      "field", field("name", $.string), $.field_kind,
      optional(seq("in", "[", commaList(field("allowed", $.string)), "]")),
      repeat(choice("heading", "compose", "inline", "optional")),
      optional(seq("[", commaList(field("target", $.string)), "]")),
      repeat(choice("heading", "compose", "inline", "optional")),
    ),

    field_kind: $ => choice(
      "text", "body",
      seq("ref", choice(field("target", $.string), seq(
        "[", commaList(field("target", $.string)), "]",
      ))),
    ),

    entity_declaration: $ => seq(
      repeat($.annotation),
      choice(seq("entity", field("type", $.string)), field("type", $.name)),
      field("id", $.entity_id),
      "{", repeat(choice($.field_value, $.braced_field, $.legacy_evidence)), "}",
    ),

    field_value: $ => seq(
      choice(seq("field", field("name", $.string)), field("name", $.name)),
      choice($.string, $.raw_string, $.reference_list),
    ),

    braced_field: $ => seq(field("name", $.braced_field_name), $.braced_body),

    reference_list: $ => seq(
      "[", optional(commaList(choice($.entity_id, $.inline_prose))), "]",
    ),

    inline_prose: $ => choice(seq($.braced_field_name, $.braced_body), seq("prose", $.raw_string)),

    legacy_evidence: $ => seq(
      "evidence", "{", repeat($.legacy_evidence_item), "}",
    ),

    legacy_evidence_item: $ => seq(
      field("kind", choice("revelation", "synthesis", "analogy")),
      "{", repeat($.field_value), "}",
    ),

    annotation: $ => seq("@", $.name, "(", $.string, ")"),

    entity_id: $ => choice($.name, $.string),
    name: _ => /[A-Za-z0-9_][A-Za-z0-9_.-]*/,
    string: _ => token(seq('"', repeat(choice(/[^"\\]/, /\\[^\r\n]/)), '"')),
    braced_body: $ => seq($.body_open, repeat($.markdown_line), $.body_close),
    raw_string: $ => seq($.raw_open, repeat($.markdown_line), $.body_close),
    line_comment: _ => token(seq("//", /[^\r\n]*/)),
    block_comment: _ => token(seq("/*", repeat(choice(/[^*]/, /\*[^/]/)), "*/")),
  },
});

function commaList(item) {
  return seq(item, repeat(seq(",", item)), optional(","));
}
