---
layout: crates.liquid
title: Classical ML & Statistics
permalink: /classical-ml
data: { hub: classical-ml }
---

Two different things share this hub. One is the scikit-learn shelf, where Rust has maintained
general-purpose toolkits covering the classic algorithms.

The other is optimization, where Rust is already load-bearing *under* Python: CVXPY, the Python
convex-modeling library, made {% assign xref = "clarabel" %}{% include "cratecard.liquid" %} its
default solver for linear and second-order cone programs, and depends on it outright{% assign ref_src = "cvxpy/cvxpy#2233" %}{% assign ref_date = "2023-09-24" %}{% assign ref_url = "https://github.com/cvxpy/cvxpy/pull/2233" %}{% assign ref_note = "CVXPY's maintainers" %}{% include "ref.liquid" %}.

A model trained in Python usually reaches Rust as an exported graph rather than as a
reimplementation; see [Model Inference & Runtimes](/inference).
