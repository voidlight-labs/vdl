# VDL for Zed

This local language extension associates `.vdl` files with VDL and highlights
packages, schemas, entity declarations, references, comments, and raw Markdown
bodies. Zed uses its Markdown parsers inside braced bodies, triple-quoted
bodies, and inline prose. Fenced code has a fallback style even without a
language tag. Emphasis uses the active theme’s comment and title styles. The editor grammar handles syntax highlighting. Use the compiler for
schema and project validation.

Build the extension with Tree-sitter CLI 0.25 and a WASI SDK:

```sh
WASI_SDK_PATH=/path/to/wasi-sdk editors/zed/build.sh
```

The script packages the compiled grammar and Zed queries in `target/zed-vdl`.
On Linux, to install the local build into a fresh extension directory:

```sh
store="${XDG_DATA_HOME:-$HOME/.local/share}/zed/extensions/installed"
test ! -e "$store/vdl" && mkdir -p "$store" && cp -R target/zed-vdl "$store/vdl"
```

Zed registers the extension from that directory. Reopen an existing buffer if
it was manually set to Plain Text. No compiler or project setting is required.
