#!/bin/sh
# TS-22-06 (ADR-022): builds the decode check with the model of the app and runs it on the report
# of `zcie usage --json`. Argument: the zcie program (default: target/debug/zcie of the repository).
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
zcie="${1:-$here/../../target/debug/zcie}"
out="$(mktemp -d)"
swiftc -O -target "$(uname -m)-apple-macos14.0" -o "$out/decode-check" "$here/Sources/Model.swift" "$here/Sources/IndexModel.swift" "$here/Tests/main.swift"
"$zcie" usage --json | "$out/decode-check"
