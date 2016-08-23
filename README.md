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
bundle exec jekyll serve
```

The site should be running on [localhost:4000](http://localhost:4000)

## Generating crate data

`_data/crates.yaml` contains a manually curated list of crates,
but the site is generated using `_data/crates_generated.yaml`
which includes additional data about each crate.
Generating updated crate data is done by running:

```
# GitHub OAuth token avoids the inevitable 403 rate limiting errors
export GITHUB_OAUTH_TOKEN=<YOUR_GITHUB_TOKEN>

# Generate data, optionally cleaning the API cache
_bin/gen_crate_data [clean]
```

TODO: Move crate generation into a [Jekyll Generator Plugin](https://jekyllrb.com/docs/plugins/#generators)