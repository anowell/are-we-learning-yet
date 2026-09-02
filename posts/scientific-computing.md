---
layout: crates.liquid
title: Numerical & Scientific Computing
permalink: /scientific-computing
data: { hub: scientific-computing }
---

Three array libraries sit under everything else here, and they are separate bodies of work: faer's
author describes `ndarray-linalg` as mostly a LAPACK wrapper, with nalgebra and faer implementing
the algorithms from scratch{% assign ref_src = "Hacker News" %}{% assign ref_date = "2024-04-24" %}{% assign ref_url = "https://news.ycombinator.com/item?id=40143669" %}{% assign ref_note = "faer's author" %}{% include "ref.liquid" %}. The Rust project has declined to bless one of them, on the
grounds that blessing one prevents competition{% assign ref_src = "rust-lang/rust-project-goals" %}{% assign ref_url = "https://github.com/rust-lang/rust-project-goals/blob/main/src/2026/high-level-ml.md" %}{% assign ref_note = "an accepted 2026 project goal" %}{% include "ref.liquid" %}.

Above them sit a scattered numerical layer, the genomics and geospatial stacks, and the interop
shelf for calling Python. Plotting is the thin spot: {% assign xref = "plotters" %}{% include "cratecard.liquid" %},
the crate most searches land on, is more or less abandoned in its maintainer's own words{% assign ref_src = "plotters-rs/plotters#702" %}{% assign ref_date = "2025-08-18" %}{% assign ref_url = "https://github.com/plotters-rs/plotters/issues/702" %}{% assign ref_note = "its maintainer" %}{% include "ref.liquid" %}.

*Statistics and optimization are under [Classical ML & Statistics](/classical-ml).*
