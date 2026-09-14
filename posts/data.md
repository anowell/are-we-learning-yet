---
layout: crates.liquid
title: Data & Dataframes
permalink: /data
data: { hub: data }
---

Arrow and Parquet are Apache specifications rather than Rust projects — the format publishes a
feature matrix across its official implementations, and Rust is one column of it{% assign ref_src = "arrow.apache.org" %}{% assign ref_url = "https://arrow.apache.org/docs/status.html" %}{% assign ref_note = "the format's own implementation matrix" %}{% include "ref.liquid" %}. Around that
column sit the query engines, columnar formats and streaming systems that are Rust top to bottom.

Several of these are Rust implementations whose documented API is the Python one, and several
others are servers you deploy rather than crates you depend on. Entries are tagged with which.
