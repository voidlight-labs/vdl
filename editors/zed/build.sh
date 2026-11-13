#!/usr/bin/env bash
set -euo pipefail

extension_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
output_dir=${1:-"$extension_dir/../../target/zed-vdl"}
tree_sitter=${TREE_SITTER_BIN:-tree-sitter}
: "${WASI_SDK_PATH:?Set WASI_SDK_PATH to the root of a WASI SDK installation}"

work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT

mkdir -p "$work/grammar/src" "$work/extension/grammars"
cp -- "$extension_dir/tree-sitter/grammar.js" "$extension_dir/tree-sitter/tree-sitter.json" "$work/grammar/"
cp -- "$extension_dir/tree-sitter/src/scanner.c" "$work/grammar/src/"
(
    cd -- "$work/grammar"
    "$tree_sitter" generate --abi 14
)
"$WASI_SDK_PATH/bin/clang" -fPIC -shared -Os \
    -Wl,--export=tree_sitter_vdl -Wl,--max-memory=4294967296 \
    -I "$work/grammar/src" "$work/grammar/src/parser.c" "$work/grammar/src/scanner.c" \
    -o "$work/extension/grammars/vdl.wasm"
cp -- "$extension_dir/extension.json" "$work/extension/"
cp -R -- "$extension_dir/languages" "$work/extension/"
mkdir -p -- "$output_dir"
cp -R -- "$work/extension/." "$output_dir/"
printf 'Built Zed extension at %s\n' "$(cd -- "$output_dir" && pwd)"
