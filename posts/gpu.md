---
layout: crates.liquid
title: GPU & Accelerators
permalink: /gpu
data: { hub: gpu }
---

This hub answers two questions. Driving a GPU from Rust is the older half, where the maintained
wrappers live; the whole pre-2022 generation below them is dead, and is listed only so you stop
finding it first.

Writing the kernel itself in Rust is the newer half, and it is moving in the open: a 2026 paper
describes GPU compilation built into `rustc` and the LLVM backends rather than bolted beside
them{% assign ref_src = "arXiv 2608.13759" %}{% assign ref_date = "2026-08-13" %}{% assign ref_url = "https://arxiv.org/abs/2608.13759" %}{% include "ref.liquid" %}, and NVIDIA publishes two Rust kernel tracks of its own while saying both are
early-stage and neither is production-ready{% assign ref_src = "NVIDIA Developer Blog" %}{% assign ref_date = "2026-09-08" %}{% assign ref_url = "https://developer.nvidia.com/blog/introducing-cuda-rust-two-tracks-for-writing-gpu-kernels/" %}{% assign ref_note = "the vendor, limiting its own product" %}{% include "ref.liquid" %}. Promising, and not yet something to stand on.
