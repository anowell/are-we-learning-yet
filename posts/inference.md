---
layout: crates.liquid
title: Model Inference & Runtimes
permalink: /inference
data: { hub: inference }
---

Running a model that was trained somewhere else is the strongest thing Rust does in machine
learning, and the community says so itself: training stays in Python, and low-latency inference is where Rust may be the right call{% assign ref_src = "users.rust-lang.org" %}{% assign ref_date = "2025-07-24" %}{% assign ref_url = "https://users.rust-lang.org/t/why-isn-t-rust-more-common-in-ai/132224" %}{% assign ref_note = "62 likes" %}{% include "ref.liquid" %}.

The usual route is an ONNX export loaded with {% assign xref = "ort" %}{% include "cratecard.liquid" %}, with pure-Rust
engines below it if you would rather not link a C++ runtime.

The exception is the small end: the 2023 generation of browser and microcontroller runtimes has
been archived or abandoned, and what works there is a pure-Rust engine compiled to WASM, on the CPU.
