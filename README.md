# are-we-learning-yet

**Rust is a systems programming language, but is it a machine learning language?**

[arewelearningyet.com](http://arewelearningyet.com)

Inspired by [Are We Web Yet?](http://arewewebyet.org/), this project aims to catalog the the Rust ML ecosystem.

## Contributing

Feedback, issues, and pull requests are welcome and appreciated for adding missing crates,
providing additional resources, or improving the content.

### Running locally

- Requires [`cobalt >=0.17.5`](https://cobalt-org.github.io/)
- Recommend [`just`](https://github.com/casey/just) as a task runner (or using the commands in the [Justfile](Justfile))


```
# GitHub OAuth token avoids 403 rate limiting errors while generated crate data
# Tip: set in `.env` file and `just` will pick it up automatically
export GITHUB_TOKEN=<YOUR_GITHUB_TOKEN>

# Scrape crate/repo data for sitegen
just scrape

# Start a dev server on port 3000
cobalt serve
```

The site should be running on [localhost:3000](http://localhost:3000)

### How it works

The repo consists of 2 key parts:
- **the scraper tool** is responsible for reading [crates.yaml](_data/crates.yaml), fetching
additional metadata about the crate from crates.io and the GitHub API, and generating a score used for ordering crates.
The fetched data is cached in a `_tmp` directory to speed up repeated site generation and avoid abusing the APIs,
and the scraper outputs `_data/crates_generated.yaml` which is used by the cobalt site generation. To force regeneration
of all crate data, remove the cached data with `just clean`. For more details, see the [scraper README](_scraper).
- **the site content** is the rest of the repo which follows the layout established by [cobalt.rs](https://cobalt-org.github.io/).
Cobalt uses this content to generate a static site into a `_site` directory (`cobalt build`) and can also start a local development
server that rebuilds the site anytime content changes (`cobalt serve`). For more details, see 
[cobalt.rs](https://cobalt-org.github.io/) documentation.

Note: `cobalt serve` does not trigger the scraper to rerun when `crates.yaml` (or the scraper) changes. Currently you must rerun `just scrape`
to update `crates_generated.yaml`.

### Publishing

[arewelearningyet.com](arewelearningyet.com) is served by Github Pages.
Every merge into master is automatically published by a [Github Actions job](.github/workflows).

Additionally, to ensure crate statistics (download counts and stars)
are regularly updated, the publishing task is also run as a weekly cron job.
