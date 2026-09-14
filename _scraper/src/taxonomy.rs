//! The site taxonomy: stack-layer hubs, each with an ordered list of sub-sections. The ids join
//! `_data/topics.yaml`, `_data/crates.yaml` and the hub pages in `posts/`, and the enum is closed,
//! so an unknown id fails the scrape rather than dropping a crate off every page it belongs on.

use anyhow::{Result, bail};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A fieldless enum serialized as the literal id given per variant, plus `ALL`, `id()` and
/// `Display`, so the id is written exactly once.
macro_rules! id_enum {
    (
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $($(#[$variant_meta:meta])* $variant:ident = $id:literal),* $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$variant_meta])* #[serde(rename = $id)] $variant),*
        }

        impl $name {
            // Not every vocabulary enumerates itself; some are only ever matched against.
            #[allow(dead_code)]
            pub const ALL: &'static [$name] = &[$($name::$variant),*];

            pub fn id(self) -> &'static str {
                match self { $($name::$variant => $id),* }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.id())
            }
        }
    };
}

pub(crate) use id_enum;

/// Declares `Hub`, `Topic` and the section-to-hub mapping from one grouped listing.
macro_rules! taxonomy {
    ($($hub:ident = $hub_id:literal { $($section:ident = $section_id:literal),* $(,)? }),* $(,)?) => {
        id_enum! { pub enum Hub { $($hub = $hub_id),* } }
        id_enum! { pub enum Topic { $($($section = $section_id,)*)* } }

        impl Topic {
            pub fn hub(self) -> Hub {
                match self { $($(Topic::$section => Hub::$hub,)*)* }
            }
        }
    };
}

taxonomy! {
    Llms = "llms" {
        LlmServing = "llm-serving",
        StructuredOutput = "structured-output",
        LlmClients = "llm-clients",
        FineTuning = "fine-tuning",
    },
    Agents = "agents" {
        AgentFrameworks = "agent-frameworks",
        AgentProtocols = "agent-protocols",
        Sandboxing = "sandboxing",
        AgentMemory = "agent-memory",
        AgentObservability = "agent-observability",
    },
    Retrieval = "retrieval" {
        VectorSearch = "vector-search",
        Embeddings = "embeddings",
        FullTextSearch = "full-text-search",
        RagPipelines = "rag-pipelines",
        DocumentAi = "document-ai",
    },
    DeepLearning = "deep-learning" {
        DlFrameworks = "dl-frameworks",
        TensorsAutodiff = "tensors-autodiff",
        TrainingInfra = "training-infra",
    },
    Inference = "inference" {
        ModelRuntimes = "model-runtimes",
        ModelPlumbing = "model-plumbing",
        EdgeInference = "edge-inference",
    },
    ClassicalMl = "classical-ml" {
        MlToolkits = "ml-toolkits",
        GradientBoosting = "gradient-boosting",
        Clustering = "clustering",
        Statistics = "statistics",
        Bayesian = "bayesian",
        TimeSeries = "time-series",
        Optimization = "optimization",
        ReinforcementLearning = "reinforcement-learning",
    },
    Data = "data" {
        Dataframes = "dataframes",
        Lakehouse = "lakehouse",
        Datasets = "datasets",
        Streaming = "streaming",
    },
    ScientificComputing = "scientific-computing" {
        ArraysLinalg = "arrays-linalg",
        NumericalMethods = "numerical-methods",
        DomainScience = "domain-science",
        Plotting = "plotting",
        PythonInterop = "python-interop",
    },
    Gpu = "gpu" {
        GpuKernels = "gpu-kernels",
        GpuRuntimes = "gpu-runtimes",
        DistributedGpu = "distributed-gpu",
    },
    Modalities = "modalities" {
        ComputerVision = "computer-vision",
        SpeechAudio = "speech-audio",
        TextProcessing = "text-processing",
        GenerativeMedia = "generative-media",
        Robotics = "robotics",
        SafetyPrivacy = "safety-privacy",
    },
}

id_enum! {
    /// The maturity verdict asserted for a sub-section, and summarized per hub. Defined on
    /// `/about/`, and in the header of `_data/topics.yaml` where they are written.
    pub enum Color {
        Green = "green",
        Yellow = "yellow",
        Red = "red",
    }
}

/// One hub, as described in `_data/topics.yaml`.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct HubDef {
    pub id: Hub,
    /// The name this hub wears on the navigation rail, where the full `title` will not fit.
    pub short: String,
    pub title: String,
    pub tagline: String,
    pub color: Color,
    pub sections: Vec<SectionDef>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SectionDef {
    pub id: Topic,
    pub title: String,
    pub color: Color,
    pub blurb: String,
    /// The sources behind `blurb`, in the order its `[1]`, `[2]` markers name them: a YAML scalar
    /// cannot hold an include, so the receipt attaches to the section as data. `xref.rs` checks
    /// both directions.
    #[serde(default)]
    pub sources: Vec<SourceDef>,
    /// When a human last held this shelf's color against the definitions in `_data/topics.yaml`.
    /// Required, and a real date -- defaulting a missing one to today is the same silence in a
    /// different costume. Nothing reads it yet: it exists so the lease length can be settled.
    pub verified: NaiveDate,
}

/// One entry in a section's `sources:` list -- the same four fields a `ref.liquid` call site
/// assigns, because both render through that template. `date` is a `NaiveDate`, so `Aug 8, 2026`
/// will not parse; whether it is in the future is a question about the world, asked in `xref.rs`.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SourceDef {
    pub src: String,
    pub url: String,
    pub date: Option<NaiveDate>,
    pub note: Option<String>,
}

/// Where the taxonomy is written down. Named here because every message in this file already points
/// the reader at it.
const TOPICS_FILE: &str = "_data/topics.yaml";

/// Checks that `_data/topics.yaml` describes exactly the taxonomy this file declares.
pub fn check_topics(topics: &[HubDef]) -> Result<()> {
    let mut hubs: HashMap<Hub, ()> = HashMap::new();
    let mut seen: HashMap<Topic, Hub> = HashMap::new();
    let mut colors: Vec<Color> = Vec::new();
    for def in topics {
        let hub = def.id;
        if hubs.insert(hub, ()).is_some() {
            bail!("hub `{hub}` is listed twice in _data/topics.yaml");
        }
        if def.sections.is_empty() {
            bail!("hub `{hub}` in _data/topics.yaml has no sections");
        }
        if def.title.trim().is_empty() || def.tagline.trim().is_empty() {
            bail!("hub `{hub}` in _data/topics.yaml needs a title and a tagline");
        }
        // The rail has one line to work with, so the length is checked and not just presence.
        let short = def.short.trim();
        if short.is_empty() || short.chars().count() > 14 {
            bail!(
                "hub `{hub}` in _data/topics.yaml needs a `short` of 1-14 characters for the \
                 navigation rail (found {:?})",
                def.short
            );
        }
        // The hub color summarizes its sections, so at least one section has to back it up.
        if !def.sections.iter().any(|s| s.color == def.color) {
            bail!(
                "hub `{hub}` in _data/topics.yaml is marked {} but none of its sections are",
                def.color
            );
        }
        let today = Utc::now().date_naive();
        for section in &def.sections {
            if section.title.trim().is_empty() || section.blurb.trim().is_empty() {
                bail!(
                    "section `{}` in _data/topics.yaml needs a title and a blurb",
                    section.id
                );
            }
            // The one kind of typo a date parser accepts happily.
            if section.verified > today {
                bail!(
                    "section `{}` in _data/topics.yaml has `verified: {}`, which is in the future",
                    section.id,
                    section.verified
                );
            }
            colors.push(section.color);
            if section.id.hub() != hub {
                bail!(
                    "section `{}` is listed under hub `{hub}` in _data/topics.yaml, but the \
                     taxonomy files it under `{}`",
                    section.id,
                    section.id.hub()
                );
            }
            if let Some(other) = seen.insert(section.id, hub) {
                bail!(
                    "section `{}` is listed twice (also under `{other}`)",
                    section.id
                );
            }
        }
    }

    for hub in Hub::ALL {
        if !hubs.contains_key(hub) {
            bail!("hub `{hub}` is missing from _data/topics.yaml");
        }
    }
    for topic in Topic::ALL {
        if !seen.contains_key(topic) {
            bail!(
                "section `{topic}` is missing from _data/topics.yaml (hub `{}`)",
                topic.hub()
            );
        }
    }

    // The blurbs' own receipts. Here rather than at the caller because this is the function that
    // owns the question "does _data/topics.yaml say something it can back up"; the rules themselves
    // live with the other source checks, in xref.rs.
    crate::xref::check_blurb_sources(Path::new(TOPICS_FILE), topics)?;

    let tally: Vec<String> = Color::ALL
        .iter()
        .map(|color| format!("{} {color}", colors.iter().filter(|c| *c == color).count()))
        .collect();
    println!(
        "Taxonomy: {} hubs, {} sections ({})",
        topics.len(),
        colors.len(),
        tally.join(", ")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(extra: &str) -> Result<SectionDef, serde_yaml_ng::Error> {
        serde_yaml_ng::from_str(&format!(
            "id: llm-serving\ntitle: LLM inference & serving\ncolor: yellow\nblurb: Engines.\n{extra}"
        ))
    }

    #[test]
    fn a_verified_date_that_is_missing_or_malformed_fails_to_parse() {
        assert!(section("verified: 2026-09-03").is_ok());
        let err = section("").unwrap_err().to_string();
        assert!(err.contains("verified"), "{err}");
        assert!(section("verified: 2026-13-45").is_err());
        assert!(section("verified: last tuesday").is_err());
        assert!(section("verified: true").is_err());
    }

    #[test]
    fn sources_are_optional_and_a_date_that_is_not_a_date_fails_to_parse() {
        // Most shelves have no sources at all, and the ones that do are checked in xref.rs.
        assert!(section("verified: 2026-09-03").unwrap().sources.is_empty());

        let with = section(
            "verified: 2026-09-03
sources:
  - src: NVIDIA
    url: https://developer.nvidia.com/blog/x
    date: 2026-09-08
    note: the vendor, limiting its own product
  - src: tract README
    url: https://github.com/sonos/tract
",
        )
        .unwrap();
        assert_eq!(with.sources.len(), 2);
        assert_eq!(with.sources[1].date, None, "a living document carries none");

        // A date that is not a date fails here rather than reaching the page.
        assert!(
            section("verified: 2026-09-03\nsources:\n  - {src: X, url: 'https://x/', date: Sep 8}")
                .is_err()
        );
    }

    #[test]
    fn a_verified_date_in_the_future_fails_the_taxonomy_check() {
        let hub = HubDef {
            id: Hub::Llms,
            short: "LLMs".into(),
            title: "LLMs & Foundation Models".into(),
            tagline: "Running pretrained models.".into(),
            color: Color::Yellow,
            sections: vec![section("verified: 2999-01-01").unwrap()],
        };
        let err = check_topics(&[hub]).unwrap_err().to_string();
        assert!(err.contains("in the future"), "{err}");
    }
}
