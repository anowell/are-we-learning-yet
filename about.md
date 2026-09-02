---
layout: page.liquid
title: About
permalink: /about/
---

<div class="prose">

**AWLY is an attempt to curate and organize the most useful Rust ML resources.**

## The colors

AWLY organizes crates into categories and shelves, and then grades **shelves** in aggregate, not the
individual crates on it: would you choose Rust for this problem, against another language? A shelf
full of excellent, production-ready crates is still yellow if together they cover a sliver of the
category. There is no formula behind it: each verdict is a judgment, made against the evidence
linked from that shelf.

<ul class="legend legend-block">
  <li><span class="swatch swatch-green"></span><span class="legend-key green">green</span>
    = <b>comprehensive</b> &mdash; covers the category, in production; you would not go wrong
    choosing Rust</li>
  <li><span class="swatch swatch-yellow"></span><span class="legend-key yellow">yellow</span>
    = <b>partial</b> &mdash; solid pieces to build on, but a sliver of the category; another
    language covers it better</li>
  <li><span class="swatch swatch-red"></span><span class="legend-key red">red</span>
    = <b>too early</b> &mdash; experimental or nonexistent; not a default choice yet</li>
</ul>

## How crates are ranked

Entries are sorted into four bands. **Featured** is a handful per topic with demonstrated large
impact &mdash; something that visibly moved a lot of people, not a well-marketed release. Those are
written by hand, with prose and independent links, and re-verified on an 18-month clock. **Listed**
clears a published bar: old enough, still active, used by people other than its author, and
described somewhere. **New & experimental** is where everything starts. **Archive** is for the
dead (yanked, archived, RustSec `unmaintained`, or two years with no release and no human commit),
kept on the page so you stop looking for them.

Downloads and stars decide nothing here; they are shown as facts. The exact clauses, thresholds and
archive reasons live in
[`_scraper/src/tier.rs`](https://github.com/anowell/are-we-learning-yet/blob/main/_scraper/src/tier.rs).

## Built with heavy AI assistance

Tracking this ecosystem comprehensively and curating it well is more work than one maintainer has,
and the site was effectively unmaintained for several years as a result. LLMs change that in both
directions: they make the effort possible to scale, and they make it easier to publish something
confidently wrong or poorly written. Significant portions of this content were generated and
reviewed by agents, then reviewed and edited by humans. All feedback is welcome — curation, content, 
emphasis, even maintenance strategy — especially from people willing to help.

AI-assisted PRs are welcome, but don't expect maintainers to do significant reading or research to
review your PR. Provide proof and provenance for content claims. Convince us you reviewed and editted
the content for things LLMs struggle with.

## Arguing with it

Everything on this site is a judgment someone can be argued out of, and the fastest way to
change one is evidence: a production user, a benchmark, a talk, a maintainer saying something on
the record. This site is maintained by [Anthony Nowell](https://github.com/anowell) and whoever
shows up, in the open, at
[github.com/anowell/are-we-learning-yet](https://github.com/anowell/are-we-learning-yet). [Report
an issue or missing content](https://github.com/anowell/are-we-learning-yet/issues/new?template=report.md)
— a missing crate, a wrong color, or poorly written content.

Inspired by [Are We Web Yet?](https://www.arewewebyet.org/), which asked the same question about a
different problem and answered it honestly.

</div>
