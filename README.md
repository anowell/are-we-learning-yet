# are-we-learning-yet

**Rust is a systems programming language, but is it a machine learning language?**

[arewelearningyet.com](http://arewelearningyet.com)

Inspired by [Are We Web Yet?](http://arewewebyet.org/), this project aims to catalog the the Rust ML ecosystem.

## Contributing

Feedback, issues, and pull requests are welcome and appreciated for adding missing crates,
providing additional resources, or more generally improving the content.

## Running locally

Running locally is just a matter of installing jekyll and serving the site:

```
bundle install

# GitHub OAuth token avoids 403 rate limiting errors while generated crate data
export GITHUB_OAUTH_TOKEN=<YOUR_GITHUB_TOKEN>

bundle exec jekyll serve
```

The site should be running on [localhost:4000](http://localhost:4000)

## Generating crate data

`_data/crates.yaml` contains a manually curated list of crates,
but the `crate_gen.rb` plugin will fetch additional data from crates.io
and the GitHub API. All fetched and generated data is cached
to speed up site generation and avoid hammerring APIs.

To force regeneration of all data, remove all cached data with `rake clean`.

## Publishing

[arewelearningyet.com](arewelearningyet.com) is served by Github Pages.
Every merge into master is automatically published by
[this Travis CI job](https://travis-ci.org/github/anowell/are-we-learning-yet).
Additionally, to ensure crate statistics (download counts and stars)
are regularly updated, the Travis publish task is also run as a weekly cron job.
