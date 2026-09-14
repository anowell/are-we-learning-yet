---
layout: redirect.liquid
title: MLOps
permalink: /mlops
data: { redirect_to: /llms, redirect_title: "LLMs & Foundation Models" }
---

There is no Rust MLOps ecosystem. There is a Rust *infrastructure* ecosystem that MLOps tools get
built out of, which is a different and more useful thing to catalog, so this page dissolved into
the layers it was really describing:

- Model servers and gateways → [LLMs & Foundation Models](/llms)
- Vector databases → [Retrieval, Embeddings & Search](/retrieval#vector-search)
- Model formats, tokenizers and hub clients → [Model Inference & Runtimes](/inference#model-plumbing)
- Pipelines and streaming → [Data & Dataframes](/data#streaming)

Orchestration, feature stores, model registries and Kubernetes operators are gaps rather than
categories in Rust.
