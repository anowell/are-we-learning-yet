# `just` stopped auto-loading dotenv files in 1.0, so opt back in explicitly
set dotenv-load := true

# Fetch crate/repo metadata and regenerate _data/crates_generated.yaml
scrape:
  cargo run --release --manifest-path _scraper/Cargo.toml -- _data/crates.yaml

# Build the static site into _site
build: scrape
  cobalt build

# Serve the site locally on port 3000
serve:
  cobalt serve --port 3000

# Check & lint the scraper the same way CI does
check:
  cargo fmt --manifest-path _scraper/Cargo.toml --all -- --check
  cargo clippy --manifest-path _scraper/Cargo.toml --all-targets -- -D warnings

# Drop generated data and the scraper's HTTP response cache
clean:
  rm -f _data/crates_generated.yaml
  rm -rf _tmp
