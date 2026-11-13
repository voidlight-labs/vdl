[(atx_heading) (setext_heading) (thematic_break)] @title.markup
[(list_marker_plus) (list_marker_minus) (list_marker_star)
 (list_marker_dot) (list_marker_parenthesis)] @punctuation.list_marker.markup
[(block_quote_marker) (block_continuation)] @punctuation.markup
(pipe_table_header "|" @punctuation.markup)
(pipe_table_row "|" @punctuation.markup)
(pipe_table_delimiter_row "|" @punctuation.markup)
(link_reference_definition) @link_text.markup
(link_destination) @link_uri.markup

[(fenced_code_block_delimiter) (info_string)] @punctuation.embedded.markup
(code_fence_content) @text.literal.markup
(indented_code_block) @text.literal.markup

((paragraph (inline) @keyword)
 (#match? @keyword "^[ \t\r\n]*include[ \t]+[A-Za-z0-9_.-]+([ \t\r\n]+include[ \t]+[A-Za-z0-9_.-]+)*[ \t\r\n]*$"))
