#!/bin/sh
# Makes a fresh copy of the ledger-rs sample project for one run of the 30-stage test.
# Usage: prepare.sh <samples dir> <target folder> <condition A|Z>
# Z: Claude Code settings of `zcie claude setup`, but no project yet: the first session makes it.
set -e
rm -rf "$2"
mkdir -p "$(dirname "$2")"
cp -R "$1/ledger-rs" "$2"
rm -rf "$2/.claude" "$2/.opencode" "$2/opencode.json"
if [ "$3" = "Z" ]; then
    zcie claude setup --dir "$2" --name "$(basename "$2")" --no-register >/dev/null
fi
