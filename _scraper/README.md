## Scraper

This scraper tool reads a yaml file listing crates, and augments that data with additional metadata from crates.io and the repository,
calculates a score used for ordering, and serializes it all back into a data file that can be consumed by the site generator.

Requires Rust 1.85 or newer (edition 2024).

### Usage

```
scraper <path/to/crates.yaml>
```

Output is always written to `_data/crates_generated.yaml`, relative to the current directory,
so run it from the repo root (`just scrape` does this for you).

`GITHUB_TOKEN` must be set; the GitHub GraphQL API rejects unauthenticated requests.

### Process details

The current process breaks down like this:

1) Parse the input yaml as a `Vec<InputCrateInfo>`
    - This now enforces that all topics in the `crates.yaml` must be variant of [`data::Topic`](src/data.rs)

2) For each crate in the input, fetches additional crate and repo metadata such as download counts or last commit timestamp.
    - Successful responses are cached in the `_tmp` directory (failures are not cached, so they are retried on the next run).
    - Crate data comes from crates.io adhering to the [crates.io scraping policy](https://crates.io/policies#crawlers) by limiting to 1 req/sec.
      crates.io only reports a license per published version, so the license of the newest stable version is resolved alongside the crate.
    - Repo data currently comes from Github via GraphQL API (where applicable). Supporting additional repo services would be a nice addition.
    - Cached data is always used if it is found. To force fetching new data, remove the cached file(s).
    - Errors with a particular crate are logged, and scraper will simply move onto the next crate.

3) The queried data is combined with the input data to generate a `Vec<GeneratedCrateInfo>`. 
    - Select fields in the input yaml take precedence over the queried values making it possible to explicitly set/override some values. See [`data::GeneratedCrateInfo::apply_overrides`](src/data.rs) for implementation details.

4) For each `GeneratedCrateInfo`, a score is calculated based on a heuristic that attempts to account for recent popularity and maintenance status.
    - The score determines how crates are sorted on the website.
    - See [`data::GeneratedCrateInfo::update_score`](src/data.rs) for implementation details.
    - Feedback is welcome for ideas on how to improve the ranking.

5) The final `Vec<GeneratedCrateInfo>` is serialized to `_data/crates_generated.yaml`.
    - Cobalt expects this in the `_data` directory to be accessible in the `site.data` variable.


### Contributor notes

- CI does run and enforce Rustfmt and clippy lints. Run `just check` (or `cargo fmt` and `cargo clippy`) prior to committing any changes.
- CI builds use a cache key containing the date for queried crate and repo data, so the CI cache is effectively cleared once per day.
