---
layout: crates.liquid
title: Agents, Tools & Context
permalink: /agents
data: { hub: agents }
---

Rust sits underneath the agent more often than it is the agent. The Model Context Protocol's
official SDK is written in Rust{% assign ref_src = "modelcontextprotocol/rust-sdk" %}{% assign ref_url = "https://github.com/modelcontextprotocol/rust-sdk" %}{% assign ref_note = "the protocol's own organization" %}{% include "ref.liquid" %}, and the isolation layer that hosted agent
sandboxes run on is Rust whether or not the product wrapped around it is.

The layers above that line are young: agent frameworks, memory, evaluation and guardrails are
mostly work of the last two years, and the cards below carry the dates.
