use anyhow::{Context, Result, bail};

mod apps;
mod collision;
mod crates;
mod data;
mod ecosystems;
mod forge;
mod http;
mod report;
mod rustsec;
mod score;
mod taxonomy;
mod tier;
mod util;
mod xref;

use crates::{CrateLookup, CratesIo};
use data::{GeneratedCrateInfo, InputCrateInfo, Measured};
use forge::{Forges, RepoLookup, RepoRef};
use http::Http;
use rustsec::Advisories;
use std::collections::HashMap;
use std::env;
use taxonomy::HubDef;
use tier::{PreviousEntry, Signals, Tier};
use util::{read_yaml, write_yaml};

/// How many failed fetches a run may carry and still be worth publishing. The failure that matters
/// arrives in bulk -- past GitHub's hourly ceiling every remaining call fails -- so this sits well
/// above a handful of 500s and an order of magnitude below an exhausted quota.
const MAX_FETCH_FAILURES: usize = 20;

const TOPICS_PATH: &str = "_data/topics.yaml";
const OUTPUT_PATH: &str = "_data/crates_generated.yaml";

/// The previous run's signals by entry id, so a failed request falls back to what it measured last
/// time rather than to zero.
fn previous_signals() -> HashMap<String, Signals> {
    // An `Err` is a first run, or a file from before this field existed: nothing to carry forward.
    read_yaml::<Vec<PreviousEntry>, _>(OUTPUT_PATH)
        .map_or_else(|_| HashMap::new(), PreviousEntry::by_id)
}

/// Resolve one repository URL and print what the forge layer makes of it.
async fn probe(url: &str) -> Result<()> {
    let url = url::Url::parse(url).context("that is not a URL")?;
    let Some(repo) = RepoRef::parse(&url) else {
        bail!(
            "no forge recognized for {url} -- supported: github.com, codeberg.org, {}",
            forge::GITLAB_HOSTS.join(", ")
        );
    };
    println!(
        "{} -> {}",
        repo.slug(),
        match &repo.forge {
            forge::Forge::GitHub => "github",
            forge::Forge::Codeberg => "codeberg (forgejo)",
            forge::Forge::GitLab { .. } => "gitlab",
        }
    );

    let forges = Forges::new(Http::new(env::var("GITHUB_TOKEN").ok())?);
    match forges.get(&repo).await {
        RepoLookup::Found(data) => println!("{data:#?}"),
        RepoLookup::Gone => println!("404 -- the forge says this repository does not exist"),
        RepoLookup::Unknown => println!("unreadable -- unknown, not a verdict"),
    }
    Ok(())
}

/// Without the non-zero exit, CI deploys a site from a catalog with most of its signals missing and
/// nothing says so.
fn check_failures(failures: usize) -> Result<()> {
    println!("Fetch failures: {failures}");
    if failures > MAX_FETCH_FAILURES {
        bail!(
            "{failures} fetches failed this run (limit {MAX_FETCH_FAILURES}). The signals behind \
             the tiers above are degraded -- rerun rather than publishing from this. A run that \
             fails this many is usually out of GitHub API quota"
        );
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args();
    let _ = args.next();
    let Some(path) = args.next() else {
        bail!(
            "Usage: scraper <path_to_crates_yaml> | scraper --probe <repository_url> \
             | scraper --curation-report <path_to_crates_generated_yaml>"
        );
    };
    if path == "--curation-report" {
        let target = args.next().unwrap_or_else(|| OUTPUT_PATH.to_string());
        print!("{}", report::render(&target)?);
        return Ok(());
    }
    if path == "--probe" {
        let Some(url) = args.next() else {
            bail!("Usage: scraper --probe <repository_url>");
        };
        return probe(&url).await;
    }

    // Two copies of the taxonomy -- this file and the enums -- so check they agree before any
    // work is done.
    let topics: Vec<HubDef> =
        read_yaml(TOPICS_PATH).with_context(|| format!("Error reading {TOPICS_PATH}"))?;
    taxonomy::check_topics(&topics).context("_data/topics.yaml disagrees with the Topic enum")?;

    let input: Vec<InputCrateInfo> =
        read_yaml(&path).with_context(|| format!("Error reading {path}"))?;
    // Before anything is fetched, so a malformed judgment fails the build in a second.
    for krate in &input {
        krate.validate().with_context(|| format!("in {path}"))?;
    }

    // A rename in the file just read above is how a reference in the prose comes to name nothing.
    let entry_ids: std::collections::HashSet<String> = input.iter().map(|k| k.id()).collect();
    xref::check(std::path::Path::new("."), &entry_ids)
        .context("a crate reference in the site's prose does not resolve")?;

    // A citation that lost its url is the page quietly becoming the source of the claim.
    xref::check_citations(std::path::Path::new("."))
        .context("a source citation in the site's prose is malformed")?;

    let previous = previous_signals();
    let now = chrono::Utc::now();

    // Without a token GitHub allows 60 requests an hour, which is not enough for the catalog.
    let github_token = env::var("GITHUB_TOKEN").ok();
    if github_token.is_none() {
        eprintln!(
            "! GITHUB_TOKEN is not set. GitHub repositories, last-real-commit dates, contributor \
             counts and the RustSec index will mostly fail; those signals will read as unknown."
        );
    }
    let forges = Forges::new(Http::new(github_token)?);
    let crates_io = CratesIo::new()?;

    let crate_names: Vec<String> = input.iter().filter_map(|k| k.name.clone()).collect();
    let advisories = match Advisories::fetch(forges.http(), &crate_names).await {
        Ok(advisories) => Some(advisories),
        Err(err) => {
            eprintln!("! RustSec advisory index unavailable ({err:#}); status stays unknown");
            None
        }
    };

    let mut generated = Vec::with_capacity(input.len());
    for krate in input {
        match (&krate.name, &krate.repository) {
            (Some(name), _) => println!("Processing crate {name}"),
            (None, Some(repo)) => println!("Processing repo {repo}"),
            (None, None) => {
                println!("Invalid entry: {krate:#?}");
                continue;
            }
        }

        let mut entry = GeneratedCrateInfo::from(&krate);
        let mut measured = Measured::default();

        if let Some(crate_name) = &krate.name {
            measured.registry_consulted = true;
            match crates_io.get_crate_data(crate_name).await {
                CrateLookup::Found(data) => {
                    measured.latest_yanked = data.latest_yanked;
                    if entry.attach_crate_data(&krate, *data) {
                        measured.removed = Some(false);
                    } else {
                        // The name resolves, but not to this project, so the registry is not a
                        // source for this entry at all -- this run or last.
                        measured.registry_consulted = false;
                        measured.latest_yanked = None;
                    }
                }
                CrateLookup::Gone => {
                    eprintln!("  ! crates.io has no crate {crate_name}");
                    measured.removed = Some(true);
                }
                CrateLookup::Unknown => {}
            }
        }
        entry.apply_overrides(&krate);

        let repo_ref = entry.repository(&krate).as_ref().and_then(RepoRef::parse);
        let mut repo_gone = false;
        if let Some(repo_ref) = &repo_ref {
            measured.forge_consulted = true;
            match forges.get(repo_ref).await {
                RepoLookup::Found(data) => entry.repo = Some(*data),
                RepoLookup::Gone => {
                    eprintln!("  ! repo {} is gone", repo_ref.slug());
                    repo_gone = true;
                }
                RepoLookup::Unknown => {}
            }
        } else if let Some(url) = entry.repository(&krate) {
            // Unknown, not unreachable: it may be alive on a host nobody has taught the scraper.
            println!("  ~ repository host not supported, repo signals stay unknown: {url}");
        }
        // A repository that 404s while the crate still resolves is a *move*, not a death.
        if repo_gone && measured.removed != Some(false) {
            measured.unreachable = Some(true);
        } else if entry.repo.is_some() {
            measured.unreachable = Some(false);
        }

        if let Some(crate_name) = entry.krate.as_ref().map(|k| k.name.clone()) {
            match ecosystems::dependent_repos(forges.http(), &crate_name).await {
                Ok(count) => measured.dependent_repos = count,
                Err(err) => eprintln!("  ! ecosyste.ms lookup failed for {crate_name}: {err:#}"),
            }
            match crates_io.reverse_dependency_owners(&crate_name).await {
                Ok((owners, total)) => {
                    measured.distinct_owner_reverse_deps = Some(owners);
                    measured.reverse_deps = Some(total);
                }
                Err(err) => eprintln!("  ! reverse dependencies failed for {crate_name}: {err:#}"),
            }
            measured.rustsec_unmaintained = advisories
                .as_ref()
                .and_then(|advisories| advisories.unmaintained(&crate_name));
        }

        entry.finish(&krate, &measured, previous.get(&entry.id), now);

        if krate.because.is_some() && krate.tier.is_none() && entry.tier != Tier::Archive {
            println!(
                "  ~ `because:` is set but nothing archives this entry (it is `{}`). The reason \
                 is recorded and inert until a trigger fires.",
                entry.tier
            );
        }

        generated.push(entry);
    }

    println!(
        "Tiers: {} entries ({})",
        generated.len(),
        tier::tally(Tier::ALL, &generated, |entry| entry.tier)
    );

    write_yaml(OUTPUT_PATH, generated)?;

    // The showcase: a second pass over the same forge layer, not a second pipeline.
    apps::run(&forges, now).await?;

    check_failures(forges.http().failures() + crates_io.failures())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `deny_unknown_fields` does not recurse, so every nested type must carry it too or a
    /// mistyped key in `_data/` is dropped in silence. One table over every hand-written type, so
    /// a new one declared without the attribute is caught here rather than by a reader noticing.
    #[test]
    fn every_hand_written_type_rejects_a_mistyped_key() {
        fn rejects<T: serde::de::DeserializeOwned>(sample: &str) {
            let name = std::any::type_name::<T>();
            serde_yaml_ng::from_str::<T>(sample).unwrap_or_else(|e| panic!("{name}: {e}"));
            let err = serde_yaml_ng::from_str::<T>(&format!("{sample}speaker: Somebody\n"))
                .err()
                .unwrap_or_else(|| panic!("{name} drops a mistyped key in silence"))
                .to_string();
            assert!(err.contains("speaker"), "{name}: {err}");
        }

        rejects::<InputCrateInfo>("name: pyo3\ntopics: [llm-serving]\n");
        rejects::<tier::Link>("kind: talk\nurl: https://example.com/talk\ntitle: A talk\n");
        rejects::<HubDef>(
            "id: gpu\nshort: GPU\ntitle: GPU\ntagline: The hardware.\ncolor: yellow\nsections: []\n",
        );
        rejects::<taxonomy::SectionDef>(
            "id: llm-serving\ntitle: Serving\ncolor: yellow\nblurb: E.\nverified: 2026-09-03\n",
        );
        rejects::<taxonomy::SourceDef>("src: NVIDIA\nurl: https://developer.nvidia.com/x\n");
        rejects::<apps::InputApp>(
            "name: Example\nrepository: https://github.com/example/example\ntopics: \
             [applications]\nrust: pure-rust\nnote: A product.\n",
        );
        rejects::<apps::AppSection>("id: applications\ntitle: Applications\n");
        rejects::<apps::CatalogLink>("url: /gpu/\ntitle: GPU crates\n");
        rejects::<apps::ClosedSource>("name: X\nurl: https://example.com\nnote: Something.\n");
    }

    #[test]
    fn a_degraded_run_fails_and_a_normal_one_does_not() {
        check_failures(0).expect("a clean run publishes");
        check_failures(MAX_FETCH_FAILURES).expect("a handful of 500s is not a degraded catalog");
        let err = check_failures(MAX_FETCH_FAILURES + 1)
            .expect_err("past the budget the run has to fail loudly")
            .to_string();
        assert!(
            err.contains("quota"),
            "the message has to say what usually causes it: {err}"
        );
    }
}
