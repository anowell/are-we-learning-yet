---
layout: crates.liquid
title: Modalities & Domains
permalink: /modalities
data: { hub: modalities }
---

The rest of this stack, pointed at images, audio, text or a robot. The same shape repeats in each:
running a model is the covered part, and the classical, non-learned half is where shelves thin out.

Speech has the most complete single answer, {% assign xref = "sherpa-onnx" %}{% include "cratecard.liquid" %}, which
ships official Rust examples for recognition and for several text-to-speech models{% assign ref_src = "k2-fsa/sherpa-onnx" %}{% assign ref_url = "https://github.com/k2-fsa/sherpa-onnx/tree/master/rust-api-examples" %}{% assign ref_note = "the toolkit's own Rust examples" %}{% include "ref.liquid" %}. Robotics
is where Rust reached into a stack it did not start in: ROS 2 lists a Zenoh middleware among its
own implementations{% assign ref_src = "ROS 2 documentation" %}{% assign ref_url = "https://github.com/ros2/ros2_documentation/blob/rolling/source/ROS-Framework/client-libraries/About-Middleware-Implementations.rst" %}{% assign ref_note = "the ROS 2 project's own list" %}{% include "ref.liquid" %}, and Zenoh is Rust.
