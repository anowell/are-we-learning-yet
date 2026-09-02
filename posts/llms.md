---
layout: crates.liquid
title: LLMs & Foundation Models
permalink: /llms
data: { hub: llms }
---

Rust is a reasonable place to run an open-weights model, and increasingly the language the thing
running it is written in: NVIDIA's Dynamo and Cloudflare's Infire are Rust-cored, and the
constrained-decoding engine several Python and C++ servers embed is a Rust crate. That is not a
recommendation to do your AI work in Rust — the highest-scoring reply in the standing thread on why
Rust is uncommon in AI says the opposite{% assign ref_src = "users.rust-lang.org" %}{% assign ref_date = "2025-07-24" %}{% assign ref_url = "https://users.rust-lang.org/t/why-isn-t-rust-more-common-in-ai/132224" %}{% assign ref_note = "the thread's highest-scoring reply" %}{% include "ref.liquid" %}.

Calling a model you did not host works for every major provider; most of those crates are
community work rather than published by the vendor whose API they wrap.

To build an application on top of a model, see [Agents, Tools & Context](/agents).
