#!/bin/sh
# Full build: sources -> dictionary pack -> WASM core -> static site in app/dist.
set -eu
cd "$(dirname "$0")/.."
./scripts/fetch.sh
cargo build --release -p builder
./target/release/builder build data/raw data/pack app/public/packs
cargo build --release -p lex-wasm --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/lex_wasm.wasm app/public/core.wasm
npm --prefix app install
npm --prefix app run build
echo "site ready in app/dist"
