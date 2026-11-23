# Lex

Offline dictionary PWA for Deutsch, English and Русский. Search runs in a Rust/WASM core over a
pack stored on the device; nothing is sent to a server.

## Layout

- `crates/core` — text normalisation, pack format and search (shared by builder and app)
- `crates/builder` — ingests the open sources and writes the pack; also a command-line query tool
- `crates/wasm` — the core compiled for the browser
- `app` — Svelte UI, dictionary worker, service worker
- `scripts/fetch.sh` — downloads the sources into `data/raw` (about 2.9 GB)

## Build

```sh
./scripts/build.sh          # everything, output in app/dist
npm --prefix app run dev    # development server
```

Query the pack from the terminal:

```sh
./target/release/builder query data/pack "gehen"
./target/release/builder bench data/pack
```

## Deploy

`app/dist` is a static site and needs HTTPS (service worker and on-device storage require it).
Pack files are split into parts of at most 4 MB, so any static host works.

For GitHub Pages, `./scripts/deploy.sh` rebuilds the site and force-pushes it as a single commit to
the `gh-pages` branch of `origin` (or of a remote URL passed as the first argument).

## Data sources

Ding (TU Chemnitz, GPL), English and Russian Wiktionary via kaikki.org and WikDict (CC BY-SA),
OpenRussian (CC BY-SA), FrequencyWords (CC BY-SA).
