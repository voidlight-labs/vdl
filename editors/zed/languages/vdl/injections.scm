((braced_body (markdown_line) @injection.content) @injection.host
  (#set! injection.language "VDL Markdown"))

((raw_string (markdown_line) @injection.content) @injection.host
  (#set! injection.language "VDL Markdown"))
