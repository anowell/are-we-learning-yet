use crate::github::RepoData;
use chrono::{DateTime, Utc};
use crates_io_api::Crate;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Serialize, Deserialize, Copy, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Topic {
    ScientificComputing,
    GpuComputing,
    NeuralNetworks,
    Metaheuristics,
    DataPreprocessing,
    DataStructures,
    Clustering,
    ComputationalCausality,
    DecisionTrees,
    LinearClassifiers,
    Reinforcement,
    Nlp,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InputCrateInfo {
    pub name: Option<String>,
    pub topics: Vec<Topic>,

    //overridable crate fields
    pub documentation: Option<String>,
    pub repository: Option<Url>,
    pub license: Option<String>,
    pub description: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct GeneratedCrateInfo {
    pub topics: Vec<Topic>,
    pub score: Option<u64>,

    #[serde(rename = "meta", skip_serializing_if = "Option::is_none")]
    pub krate: Option<Crate>,

    #[serde(rename = "repo", skip_serializing_if = "Option::is_none")]
    pub repo: Option<RepoData>,
}

impl From<&InputCrateInfo> for GeneratedCrateInfo {
    fn from(input: &InputCrateInfo) -> Self {
        GeneratedCrateInfo {
            topics: input.topics.clone(),
            score: None,
            krate: None,
            repo: None,
        }
    }
}

// New value takes precedent if it exists
fn replace_opt<T: Clone>(original: &mut Option<T>, extra: &Option<T>) {
    if let Some(val) = extra {
        let _ = original.replace(val.clone());
    }
}

// Helper to allow specific fields from crates.yml
// to override the values returned by the Crates.io API
pub fn override_crate_data(krate: &mut Crate, input: &InputCrateInfo) {
    replace_opt(&mut krate.license, &input.license);
    replace_opt(&mut krate.documentation, &input.documentation);
    if krate.documentation.is_none() {
        krate.documentation = Some(format!("https://docs.rs/crate/{}", krate.name));
    }
    replace_opt(
        &mut krate.repository,
        &input.repository.as_ref().map(|r| r.to_string()),
    );
    replace_opt(&mut krate.description, &input.description);
}

impl GeneratedCrateInfo {
    //   In calculating last_activity, we only scrape last_commit for github-based crates
    //   so this is unfair to projects that host source elsewhere.
    //   This is slightly mitigated by falling back to the last crate publish date
    fn last_activity(&self) -> Option<DateTime<Utc>> {
        let mut last_activity = self.krate.as_ref().map(|k| k.updated_at);

        if let Some(last_commit) = self.repo.as_ref().map(|r| r.last_commit) {
            if Some(last_commit) > last_activity {
                last_activity = Some(last_commit);
            }
        }

        last_activity
    }

    pub fn update_score(&mut self) {
        if self.krate.is_none() {
            // crate is not published to crates.io
            self.score = Some(0);
        }

        let coefficient = match self.last_activity() {
            None => 0.1,
            Some(last_activity) => {
                let inactive_days = (Utc::now() - last_activity).num_days();

                // This is really simple, but basically calls any crate with activity in 6 months as maintained
                // trying to recognize that some crates may actually be stable enough to require infrequent changes
                // From 6-12 months, it's maintenance state is less certain, and after a year without activity, it's likely unmaintained
                if inactive_days <= 180 {
                    1.0
                } else if inactive_days <= 365 {
                    0.5
                } else {
                    0.1
                }
            }
        };

        let recent_downloads = self
            .krate
            .as_ref()
            .and_then(|k| k.recent_downloads)
            .unwrap_or(0);
        self.score = Some(f32::floor(coefficient * recent_downloads as f32) as u64);
    }
}
