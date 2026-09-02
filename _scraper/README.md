## Scraper

This scraper tool reads a yaml file listing crates, augments that data with metadata from crates.io,
the project's repository host, ecosyste.ms and the RustSec advisory database, applies the curation
model in [`tier.rs`](src/tier.rs), calculates a score used for ordering, and serializes it all back
into a data file that can be consumed by the site generator.

Requires Rust 1.85 or newer (edition 2024).

### Usage

```
scraper <path/to/crates.yaml>
scraper --probe <repository-url>            # resolve one repo through the forge layer and print it
scraper --curation-report [generated.yaml]  # what curation needs re-checked, as markdown
```

Output is always written to `_data/crates_generated.yaml`, relative to the current directory,
so run it from the repo root (`just scrape` does this for you).

`GITHUB_TOKEN` must be set; unauthenticated GitHub API requests get rate limited almost immediately.
Without it the run still completes, but repository, last-real-commit, contributor and RustSec
signals all read as unknown, and unknown signals neither list nor archive anything.

### Process details

The current process breaks down like this:

0) Parse and cross-check the taxonomy: `_data/topics.yaml` against [`taxonomy::Hub`/`taxonomy::Topic`](src/taxonomy.rs).
    - The taxonomy lives in two places, code and data, and they must agree: a hub or section that is in one
      and not the other fails the run, as does a section listed under the wrong hub. Otherwise a typo means
      a crate silently disappears from every page it belonged on.

1) Parse the input yaml as a `Vec<InputCrateInfo>`
    - This enforces that every `topics` entry in `crates.yaml` is a `taxonomy::Topic` (a sub-section id from
      `_data/topics.yaml`), and that no entry is topic-less.

2) For each crate in the input, fetches additional crate and repo metadata such as download counts or last commit timestamp.
    - Successful responses are cached in the `_tmp` directory. Failures -- timeouts, 5xx, rate limits -- are
      not cached, so they are retried on the next run. A **404 is cached for 24 hours** and no longer: it is
      the one cached status that feeds an archive trigger, so a repository that went private for an afternoon
      has to be able to un-archive itself. See `http::NOT_FOUND_TTL`.
    - Repository hosts are rate-limit aware: a 403 or 429 carrying `X-RateLimit-Remaining: 0` waits for the
      window named by `X-RateLimit-Reset` (or `Retry-After`) instead of failing every remaining call, up to
      `http::MAX_RATE_LIMIT_WAIT`.
    - Crate data comes from crates.io adhering to the [crates.io scraping policy](https://crates.io/policies#crawlers) by limiting to 1 req/sec.
      crates.io only reports a license per published version, so the license of the newest stable version is resolved alongside the crate.
    - Repo data comes from GitHub, Codeberg (Forgejo) or one of the GitLab instances in
      `forge::GITLAB_HOSTS` -- see [`forge.rs`](src/forge.rs).
      A host nobody has taught the scraper leaves every repo-derived signal *unknown*, which is not the same
      as dead: the project may be perfectly alive on sourcehut. `just probe <url>` says what the forge layer
      makes of a single URL.
    - The maintenance signals are what the curation model runs on: last commit **by a human** (the commits API
      with bot authors filtered out -- never `pushed_at`, which moves for bot pushes and PR branches and so
      reports frozen projects as maintained), contributor counts, the owner's archived
      flag, yank status, reverse dependencies **by distinct owner**, and RustSec
      `informational = "unmaintained"`. ecosyste.ms `dependent_repos_count` is fetched and rendered but
      is in neither the bar nor the ranking: it counts lockfile depth rather than use
      (`matrixmultiply` 6975, `candle-core` 1).
    - Reverse-dependency owner resolution is the expensive one: one request per dependent, at one request a
      second. It is budgeted (see `crates::OWNER_BUDGET`) and the resulting count is a floor.
    - A signal that could not be fetched falls back to the **previous run's** value from
      `_data/crates_generated.yaml`, never to zero. A transient 500 must not be able to archive a live project.
      (This depends on that file surviving between runs, which is a property of the CI cache, not of the
      scraper.)
    - Cached data is always used if it is found. To force fetching new data, remove the cached file(s).
    - Errors with a particular crate are logged, and scraper will simply move onto the next crate -- but they
      are also **counted**, and a run that fails more than `main::MAX_FETCH_FAILURES` of them exits non-zero
      rather than publishing a catalog whose maintenance signals are mostly unknown. The count is printed
      either way.

3) The queried data is combined with the input data to generate a `Vec<GeneratedCrateInfo>`. 
    - Select fields in the input yaml take precedence over the queried values making it possible to explicitly set/override some values. See [`data::GeneratedCrateInfo::apply_overrides`](src/data.rs) for implementation details.

4) The curation model in [`tier.rs`](src/tier.rs) decides the tier: `featured`, `listed`, `watch` or
   `archive`. Only `featured` is ever written by a human; the other three are computed. Archiving in
   particular is automatic -- a human writes the *reason* (`because:`) and the `replacement:`, which
   rename an automatic archive but never cause one.
    - Every archive trigger fires on positive evidence only, and every clause of the published bar that cannot
      be evaluated is recorded as *unknown* rather than counted as a failure. A missing signal never archives
      a live crate and never demotes one.

5) For each `GeneratedCrateInfo`, a score is calculated. It determines the ordering within a tier.
    - **There is no download term.** crates.io recent-download counts are un-deduplicated CDN logs:
      `instant-distance` has 702k/90d and an archived repo, `plotters` carries criterion's 48M badge.
      Downloads and stars still render on the card as facts.
    - In weight order: distinct-owner reverse dependencies, recency of the last real commit, age, and
      stars purely as a tie-break; the weights themselves are in `score::weight`. A repo-only entry has
      no registry to be depended on in, so its contributor count stands in for the adoption term.
    - See [`score.rs`](src/score.rs) for implementation details.

6) The final `Vec<GeneratedCrateInfo>` is serialized to `_data/crates_generated.yaml`.
    - Cobalt expects this in the `_data` directory to be accessible in the `site.data` variable.


### Contributor notes

- CI does run and enforce Rustfmt and clippy lints. Run `just check` (or `cargo fmt` and `cargo clippy`) prior to committing any changes.
- CI builds use a cache key containing the date for queried crate and repo data, so the CI cache is effectively cleared once per day.
  `_tmp/owners` is excluded from that key and cached on a rolling one instead (`.github/workflows/build.yml`),
  because crate ownership changes about never and re-fetching those lookups daily cost roughly half an hour.
- `--curation-report` runs on every build and writes to the run summary: the `featured` leases about to
  lapse, the archives asserted by hand, and the entries flagged `finished`. It changes nothing.
