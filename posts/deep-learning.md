---
layout: crates.liquid
title: Deep Learning & Training
permalink: /deep-learning
data: { hub: deep-learning }
---

Pure-Rust training means Burn; candle can train but is strongest at inference, and tch binds
libtorch. Everything else here is historic or a one-maintainer experiment.

Where the community lands is that training stays where the ecosystem already is{% assign ref_src = "users.rust-lang.org" %}{% assign ref_date = "2025-07-24" %}{% assign ref_url = "https://users.rust-lang.org/t/why-isn-t-rust-more-common-in-ai/132224" %}{% assign ref_note = "62 likes" %}{% include "ref.liquid" %}. What Rust
offers is the other end of the job: a training binary with no Python runtime inside it.

To run a model someone else trained, see [Model Inference & Runtimes](/inference).
