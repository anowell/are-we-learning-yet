//! Crate names that mean something other than a reader of this site would assume: `- name: Kyanite`
//! fetches a gallery collector on GitLab, not the ONNX inference library. A name on this list must
//! pin `repository:` in `_data/crates.yaml` and the pin must agree with the registry, or no
//! registry data is attached. Elsewhere a disagreement is only reported: projects move.

use crate::forge::RepoRef;
use url::Url;

/// Names where the crates.io entry is not the project the catalog means: an unrelated crate, a
/// squat, a stub, or a former owner's abandoned name. Each comment says what the registry actually
/// holds under the name.
pub const COLLISIONS: &[&str] = &[
    "cuda-oxide",   // an unrelated dead 2021 crate, not NVIDIA Labs' project
    "nccl",         // a configuration language
    "descend",      // a squat
    "emu",          // not the GPGPU crate a reader expects (that one is `emu_core`)
    "ratchet-core", // a WebSocket library, not Hugging Face's ratchet
    "vector",       // unrelated to Vector the observability product
    "llm",          // now graniet's live project, not the archived rustformers one
    "tabpfn",       // an empty placeholder
    "arroyo",
    "risingwave",
    "feldera",
    // a gallery collector on GitLab; KarelPeeters/Kyanite publishes as `kn-graph`/`kn-cuda-eval`
    "Kyanite",
    "copper-rs", // devtaube/copper.rs, a 2D games library (2023); the robotics runtime is `cu29`
    "dynamo",    // a PistonDevelopers scripting language (2016), not NVIDIA's serving stack
    "furnace",   // Kroisse/furnace, a React-inspired GUI library (2019), not the Burn server
    "brush",     // reubeno/brush, a POSIX/bash shell; the Gaussian-splatting project is unpublished
    "crane",     // timothebot/crane, a project-scaffolding tool, not lucasjinreal's LLM engine
    "daft",      // oxidecomputer/daft, structural diffs; Daft the data engine ships no engine crate
    "anda",      // FyraLabs' Andaman build toolchain, not ldclabs' agent framework
    "optuna",    // v0.0.0-alpha.1 with no repository -- a squat on the Python project's name
    "shap",      // a v0.1.0 stub whose description is the word "shap" and which exports one fn
    "arkflow",   // chenquan/arkflow v0.1.0 (2025), not the arkflow-rs stream processor
    "rivers",    // tylerhorton/rivers, a 2022 streams library, not ion-elgreco's ML pipeline tool
    // newfla's stable-diffusion.cpp bindings; EricLBuehler's dead project of the same name was
    // never published, so the pin is what keeps the right one attached
    "diffusion-rs",
];

pub fn is_collision(crate_name: &str) -> bool {
    COLLISIONS
        .iter()
        .any(|name| name.eq_ignore_ascii_case(crate_name))
}

/// Whether two repository URLs name the same project, compared on forge identity rather than
/// string equality. A URL on a host the scraper cannot parse compares equal only to itself.
pub fn same_repo(a: &Url, b: &Url) -> bool {
    match (RepoRef::parse(a), RepoRef::parse(b)) {
        (Some(a), Some(b)) => a.slug() == b.slug(),
        _ => a
            .as_str()
            .trim_end_matches('/')
            .eq_ignore_ascii_case(b.as_str().trim_end_matches('/')),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    /// Case-insensitive but exact: several of the listed names are short and generic, and the real
    /// crates the catalog lists under adjacent names must not be caught by them.
    #[test]
    fn a_collision_matches_the_whole_name_ignoring_case() {
        assert!(is_collision("Kyanite"));
        assert!(is_collision("kyanite"));
        assert!(!is_collision("emu_core"), "the real GPGPU crate");
        assert!(!is_collision("shap-rs"), "the real SHAP crate");
        assert!(!is_collision("cu29"), "the real copper-rs crate");
        assert!(!is_collision("dynamo-runtime"), "NVIDIA's published crate");
    }

    #[test]
    fn repository_identity_survives_cosmetic_differences() {
        assert!(same_repo(
            &url("https://github.com/calebwin/emu"),
            &url("https://www.github.com/calebwin/emu")
        ));
        assert!(same_repo(
            &url("https://github.com/rust-ml/linfa.git"),
            &url("https://github.com/rust-ml/linfa/tree/master/algorithms")
        ));
        assert!(!same_repo(
            &url("https://github.com/KarelPeeters/Kyanite"),
            &url("https://gitlab.com/lu-ci/kyanite")
        ));
    }
}
