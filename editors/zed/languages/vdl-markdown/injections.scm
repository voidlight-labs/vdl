([
  (inline)
  (pipe_table_cell)
] @injection.content
 (#set! injection.language "VDL Markdown Inline"))

(fenced_code_block
  (info_string (language) @injection.language)
  (code_fence_content) @injection.content)
