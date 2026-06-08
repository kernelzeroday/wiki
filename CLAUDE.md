# wiki

Rust CLI for searching and reading Wikipedia from the terminal.

## Architecture

- Async HTTP via `reqwest` + `tokio`, targeting the Wikipedia REST API
- `src/main.rs` — CLI argument parsing (clap derive), command routing, command handlers
- `src/api.rs` — Wikipedia API client struct with typed serde responses
- `src/display.rs` — Output formatting, HTML rendering via `html2text`, colored output

## Build & install

```bash
cargo build --release
codesign -s - target/release/wiki   # ad-hoc sign for macOS
cp target/release/wiki ~/bin/
```

## Usage

```bash
wiki search "quantum computing"
wiki summary "Haskell (programming language)"
wiki page "Linux" -w 120
wiki random
wiki -l de search "Philosophie"    # other languages
```

## API

Uses Wikipedia REST API (`/api/rest_v1/` and `/w/rest.php/v1/`):
- Search: `GET /w/rest.php/v1/search/page?q={query}`
- Summary: `GET /api/rest_v1/page/summary/{title}`
- Full page: `GET /api/rest_v1/page/html/{title}`
- Random: `GET /api/rest_v1/page/random/summary`
