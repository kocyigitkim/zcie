#!/bin/sh
# Builds the npm package of ZCIE for this computer (ADR-014).
# Usage: scripts/packaging/build-npm.sh
# Output: dist/zcie-<version>.tgz. Install it with: npm install -g dist/zcie-<version>.tgz
# The package has the release programs zcie, zcied and zcie-mcp of this platform in
# native/<platform>-<arch>/; `os` and `cpu` in package.json name the platforms of the package.
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
platform="$(node -p 'process.platform + "-" + process.arch')"
ext=""
case "$platform" in win32-*) ext=".exe" ;; esac  # Windows: run in Git Bash (ADR-027)
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
cargo build --release --bin zcie --bin zcied --bin zcie-mcp
stage="$root/dist/npm/zcie"
rm -rf "$stage"
mkdir -p "$stage/native/$platform"
cp -R npm/zcie/bin npm/zcie/README.md npm/zcie/package.json "$stage/"
for program in zcie zcied zcie-mcp; do
    cp "target/release/$program$ext" "$stage/native/$platform/$program$ext"
    chmod 755 "$stage/native/$platform/$program$ext"
done
# Set the version of the Rust workspace and the platforms that the package has.
node -e '
const fs = require("fs");
const [file, version, stage] = process.argv.slice(1);
const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
const platforms = fs.readdirSync(stage + "/native");
pkg.version = version;
pkg.os = [...new Set(platforms.map((p) => p.split("-")[0]))];
pkg.cpu = [...new Set(platforms.map((p) => p.split("-")[1]))];
fs.writeFileSync(file, JSON.stringify(pkg, null, 2) + "\n");
' "$stage/package.json" "$version" "$stage"
npm pack "$stage" --pack-destination "$root/dist" >/dev/null
echo "$root/dist/zcie-$version.tgz"
