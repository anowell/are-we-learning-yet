//! The showcase: finished AI products with a Rust core, listed from repository metrics. Stars are
//! the bar here and nowhere else on the site, because nobody takes a dependency on a terminal, so
//! the reverse-dependency signal `tier.rs` ranks on reads zero here whatever the adoption.

use crate::forge::{Forges, RepoData, RepoLookup, RepoRef};
use crate::taxonomy::id_enum;
use crate::tier::{self, ArchiveReason, BarResult, Link, PreviousEntry, Signals};
use crate::util::{read_yaml, write_yaml};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

const INPUT_PATH: &str = "_data/apps.yaml";
const OUTPUT_PATH: &str = "_data/apps_generated.yaml";

/// `ACTIVITY_DAYS` is deliberately not re-declared here: an app going quiet and a crate going quiet
/// mean the same thing.
mod threshold {
    /// The listing bar. Missing it means `watch`; nothing here archives or hides anything.
    pub const MIN_STARS: u32 = 5_000;
}

id_enum! {
    /// The sections of the showcase. Closed: an id that is not here fails the scrape rather than
    /// filing an entry onto no page at all.
    pub enum AppTopic {
        /// Agents, editors, terminals — software developers run to write software.
        AgentsDevtools = "agents-devtools",
        /// Products whose users are not necessarily programmers.
        Applications = "applications",
        /// Models and the inference stacks under them, where the Rust is the serving path.
        ModelsInference = "models-inference",
    }
}

id_enum! {
    /// What the Rust in this product actually is. A human's call: a TypeScript frontend over a Rust
    /// ML pipeline is mostly TypeScript by weight and mostly Rust by the fact a reader came for.
    pub enum RustExtent {
        /// Rust from the core to the interface. No other language is load-bearing.
        PureRust = "pure-rust",
        /// A Rust core under a shell, UI or training stack written in something else.
        RustCore = "rust-core",
    }
}

id_enum! {
    /// Where an entry sits on the showcase: the catalog's vocabulary minus `featured`.
    pub enum AppTier {
        Listed = "listed",
        /// Below the bar. Still on the page, collapsed, as in the catalog.
        Watch = "watch",
        /// Automatic, and it reverses itself when the fact changes.
        Archive = "archive",
    }
}

id_enum! {
    /// The part of `AppTier` a human may assert in `_data/apps.yaml`. One variant, on purpose.
    pub enum ManualAppTier {
        Archive = "archive",
    }
}

/// A cross-reference from a showcase section into the catalog. Not evidence, so it carries no
/// `kind`.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CatalogLink {
    pub url: String,
    pub title: String,
}

/// One section heading, read by the layout straight out of `_data/apps.yaml`.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct AppSection {
    pub id: AppTopic,
    pub title: String,
    #[serde(default)]
    pub catalog: Vec<CatalogLink>,
}

/// A Rust AI product with nothing public to measure. Deliberately holds no field the scraper could
/// have fetched, so no figure here exists in two places and updates in one.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ClosedSource {
    pub name: String,
    /// Where the claim was published. The entry's name links to it.
    pub url: Url,
    /// What it is and what was claimed about it, version-scoped where the claim is a number.
    pub note: String,
}

impl ClosedSource {
    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("a closed-source entry in {INPUT_PATH} has no `name:`");
        }
        if self.note.trim().is_empty() {
            bail!("closed-source entry `{}` has no `note:`", self.name);
        }
        if !matches!(self.url.scheme(), "http" | "https") {
            bail!(
                "closed-source entry `{}` has a `url:` that is not a public URL ({})",
                self.name,
                self.url
            );
        }
        Ok(())
    }
}

/// `_data/apps.yaml`: the sections, then the entries, then the closed-source residue.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AppFile {
    pub sections: Vec<AppSection>,
    pub apps: Vec<InputApp>,
    #[serde(default)]
    pub closed_source: Vec<ClosedSource>,
}

/// One showcase entry as a human wrote it: judgment and corrections only, as in
/// `_data/crates.yaml`.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct InputApp {
    /// What the product is called, which is rarely what its repository is called.
    pub name: String,
    /// Required: an entry with nothing to measure belongs in `closed_source` instead.
    pub repository: Url,
    pub topics: Vec<AppTopic>,
    pub rust: RustExtent,
    /// Where the Rust is and what it does.
    pub note: String,
    /// Independent evidence, in the catalog's vocabulary: a talk, a post, a benchmark, a paper.
    #[serde(default)]
    pub links: Vec<Link>,

    // --- the archive vocabulary, identical to the catalog's ---
    pub tier: Option<ManualAppTier>,
    pub status_date: Option<NaiveDate>,
    pub because: Option<ArchiveReason>,
    pub evidence: Option<Url>,
    pub replacement: Option<String>,
    pub verified_by: Option<String>,
    /// This product is complete, not abandoned. Opts out of the dormancy trigger.
    #[serde(default)]
    pub finished: bool,
}

impl InputApp {
    /// The repository slug, which is this entry's identity across runs.
    fn id(&self) -> String {
        RepoRef::parse(&self.repository).map_or_else(|| self.repository.to_string(), |r| r.slug())
    }

    fn validate(&self) -> Result<()> {
        let id = self.id();

        if self.name.trim().is_empty() {
            bail!("{id} has no `name:`");
        }
        if self.topics.is_empty() {
            bail!("{id} lists no topics, so it would not appear in any section");
        }
        if self.note.trim().is_empty() {
            bail!(
                "{id} has no `note:`. Every entry here says where the Rust is and what it does; \
                 a name and a star count on their own are a leaderboard row"
            );
        }
        if RepoRef::parse(&self.repository).is_none() {
            bail!(
                "{id} has a `repository:` on a host the scraper cannot read, so none of this \
                 page's numbers could be measured for it. Supported: github.com, codeberg.org, {}",
                crate::forge::GITLAB_HOSTS.join(", ")
            );
        }
        if let Some(date) = self.status_date
            && date > Utc::now().date_naive()
        {
            bail!("{id} has a status_date in the future ({date})");
        }
        if let Some(url) = &self.evidence
            && !matches!(url.scheme(), "http" | "https")
        {
            bail!("{id} has `evidence:` that is not a public URL ({url})");
        }
        if self.evidence.is_some() && self.because.is_none() {
            bail!("{id} cites `evidence:` with no `because:` for it to be evidence of");
        }
        if let Some(reason) = self.because
            && self.tier == Some(ManualAppTier::Archive)
            && reason.is_detected()
        {
            bail!(
                "{id} asserts `tier: archive` with `because: {reason}`, which the scraper detects \
                 for itself on every run. Drop the `tier:` line -- the trigger archives it, and \
                 un-archives it if the fact changes"
            );
        }
        if self.tier == Some(ManualAppTier::Archive) {
            if self.because.is_none() {
                bail!(
                    "{id} is archived without `because:`. Pick one the scraper cannot detect for \
                     itself: {}",
                    ArchiveReason::ALL
                        .iter()
                        .filter(|reason| !reason.is_detected())
                        .map(|reason| reason.id())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            if self.evidence.is_none() {
                bail!("{id} is archived without `evidence:`. Cite something public");
            }
            if self.status_date.is_none() {
                bail!("{id} is archived without `status_date:` saying when that was decided");
            }
        }
        if self.finished {
            if self.verified_by.is_none() {
                bail!(
                    "{id} sets `finished: true` without `verified_by:`. It opts the entry out of \
                     the dormancy trigger permanently, so it needs a name on it"
                );
            }
            if self.status_date.is_none() {
                bail!("{id} sets `finished: true` without `status_date:` saying when");
            }
        }
        for link in &self.links {
            if link.title.trim().is_empty() {
                bail!("{id} has a link with no title ({})", link.url);
            }
            if !link.url.starts_with("http://") && !link.url.starts_with("https://") {
                bail!("{id} has a link that is not a public URL ({})", link.url);
            }
        }
        Ok(())
    }
}

/// Written into the generated file so the page can state the numbers it enforces rather than
/// restating them in prose.
#[derive(Serialize)]
pub struct PublishedBar {
    pub min_stars: u32,
    pub activity_days: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct GeneratedApp {
    pub id: String,
    pub name: String,
    pub topics: Vec<AppTopic>,
    pub rust: RustExtent,
    pub tier: AppTier,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub because: Option<ArchiveReason>,
    pub auto_archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_by: Option<String>,
    pub finished: bool,

    pub note: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,

    pub signals: Signals,
    pub bar: BarResult,
    /// Sort key only: stars, or 0 for a repository nobody could read. `signals.stars` keeps the
    /// unknown.
    pub score: u64,

    #[serde(rename = "repo", skip_serializing_if = "Option::is_none")]
    pub repo: Option<RepoData>,
}

/// `_data/apps_generated.yaml`. A mapping rather than a bare list, so the bar the page prints has
/// somewhere to live instead of being typed twice.
#[derive(Serialize)]
pub struct GeneratedApps {
    pub bar: PublishedBar,
    pub apps: Vec<GeneratedApp>,
}

/// The last run's file, read back for its signals alone.
#[derive(Deserialize)]
struct PreviousApps {
    #[serde(default)]
    apps: Vec<PreviousEntry>,
}

fn previous_signals() -> HashMap<String, Signals> {
    read_yaml::<PreviousApps, _>(OUTPUT_PATH)
        .map_or_else(|_| HashMap::new(), |file| PreviousEntry::by_id(file.apps))
}

/// Checks that `_data/apps.yaml` describes exactly the sections this file declares.
fn check_sections(sections: &[AppSection]) -> Result<()> {
    let mut seen: HashMap<AppTopic, ()> = HashMap::new();
    for section in sections {
        if section.title.trim().is_empty() {
            bail!("section `{}` in {INPUT_PATH} needs a title", section.id);
        }
        if seen.insert(section.id, ()).is_some() {
            bail!("section `{}` is listed twice in {INPUT_PATH}", section.id);
        }
        for link in &section.catalog {
            if link.title.trim().is_empty() || link.url.trim().is_empty() {
                bail!(
                    "section `{}` has a catalog link with no title or no url",
                    section.id
                );
            }
        }
    }
    for topic in AppTopic::ALL {
        if !seen.contains_key(topic) {
            bail!("section `{topic}` is missing from {INPUT_PATH}");
        }
    }
    Ok(())
}

/// The published bar for this page, shaped like `tier::evaluate_bar`: unknown is recorded and never
/// counted as a failure, and nothing here feeds an archive trigger.
fn evaluate_bar(signals: &Signals, now: DateTime<Utc>) -> BarResult {
    let mut bar = BarResult::default();

    bar.check("public_repo", Some(signals.has_repo));
    bar.check(
        "repo_not_archived",
        signals.repo_archived.map(|flagged| !flagged),
    );
    bar.check("stars", signals.stars.map(|n| n >= threshold::MIN_STARS));

    // No release route: an app is not published to a registry, so this is the commit test alone.
    bar.check(
        "recent_activity",
        tier::any_of([
            Some(signals.finished),
            tier::older_than(
                signals.last_real_commit,
                tier::threshold::ACTIVITY_DAYS,
                now,
            )
            .map(|stale| !stale),
        ]),
    );

    bar.met = bar.failed.is_empty();
    bar
}

#[derive(Debug)]
pub struct AppVerdict {
    pub tier: AppTier,
    pub because: Option<ArchiveReason>,
    pub auto_archived: bool,
    pub bar: BarResult,
}

/// The catalog's ladder, one step shorter: there is no endorsement here to outrank dormancy, so
/// the trigger always applies.
pub fn decide(
    tier_asserted: Option<ManualAppTier>,
    because: Option<ArchiveReason>,
    signals: &Signals,
    now: DateTime<Utc>,
) -> AppVerdict {
    let bar = evaluate_bar(signals, now);
    let manual_archive = tier_asserted == Some(ManualAppTier::Archive);

    if let Some((because, auto_archived)) =
        tier::archived_because(manual_archive, because, true, signals, now)
    {
        return AppVerdict {
            tier: AppTier::Archive,
            because,
            auto_archived,
            bar,
        };
    }

    let tier = if bar.met {
        AppTier::Listed
    } else {
        AppTier::Watch
    };
    AppVerdict {
        tier,
        because: None,
        auto_archived: false,
        bar,
    }
}

/// What the forge said, reduced to the signals the decision reads. Absent means unknown, never no:
/// a bad afternoon at the forge must not empty this page.
fn signals_for(input: &InputApp, repo: Option<&RepoData>, gone: bool) -> Signals {
    Signals {
        // A fact about an app, not a missing measurement: there is no registry in this pipeline.
        on_crates_io: false,
        has_repo: true,
        repo_archived: repo.and_then(|r| r.archived),
        // `removed:` stays unset for the same reason `on_crates_io` is false: no registry to be
        // removed from. A 404 with no registry behind it is `unreachable`, as in the catalog.
        unreachable: if gone {
            Some(true)
        } else {
            repo.map(|_| false)
        },
        first_published: repo.and_then(|r| r.created_at),
        last_real_commit: repo.and_then(|r| r.last_real_commit),
        last_push: repo.and_then(|r| r.last_commit),
        contributors: repo.and_then(|r| r.contributors),
        stars: repo.map(|r| r.stargazers_count),
        // Recorded, but not a bar clause: `note:` is required on every entry here.
        has_description: repo.map(|r| r.description.is_some()),
        finished: input.finished,
        ..Signals::default()
    }
}

/// Read `_data/apps.yaml`, measure every entry, write `_data/apps_generated.yaml`.
pub async fn run(forges: &Forges, now: DateTime<Utc>) -> Result<()> {
    let file: AppFile =
        read_yaml(INPUT_PATH).with_context(|| format!("Error reading {INPUT_PATH}"))?;
    check_sections(&file.sections)
        .with_context(|| format!("{INPUT_PATH} disagrees with the AppTopic enum"))?;
    for app in &file.apps {
        app.validate().with_context(|| format!("in {INPUT_PATH}"))?;
    }
    for entry in &file.closed_source {
        entry
            .validate()
            .with_context(|| format!("in {INPUT_PATH}"))?;
    }

    let previous = previous_signals();
    let mut generated = Vec::with_capacity(file.apps.len());

    for input in &file.apps {
        let id = input.id();
        println!("Processing app {} ({id})", input.name);

        let repo_ref = RepoRef::parse(&input.repository).expect("checked by validate");
        let mut repo = None;
        let mut gone = false;
        match forges.get(&repo_ref).await {
            RepoLookup::Found(data) => repo = Some(*data),
            RepoLookup::Gone => {
                eprintln!("  ! repo {id} is gone");
                gone = true;
            }
            RepoLookup::Unknown => eprintln!("  ! repo {id} unreadable; signals stay unknown"),
        }

        let mut signals = signals_for(input, repo.as_ref(), gone);
        if let Some(previous) = previous.get(&id) {
            // Only the forge feeds this pipeline, so only the forge half carries forward.
            signals.or_previous(previous, false, true);
        }

        let verdict = decide(input.tier, input.because, &signals, now);
        generated.push(GeneratedApp {
            id,
            name: input.name.clone(),
            topics: input.topics.clone(),
            rust: input.rust,
            tier: verdict.tier,
            because: verdict.because,
            auto_archived: verdict.auto_archived,
            status_date: input.status_date,
            evidence: input.evidence.as_ref().map(Url::to_string),
            replacement: input.replacement.clone(),
            verified_by: input.verified_by.clone(),
            finished: input.finished,
            note: input.note.clone(),
            links: input.links.clone(),
            score: u64::from(signals.stars.unwrap_or(0)),
            signals,
            bar: verdict.bar,
            repo,
        });
    }

    println!(
        "Showcase: {} apps at {} stars ({}), {} closed-source",
        generated.len(),
        threshold::MIN_STARS,
        tier::tally(AppTier::ALL, &generated, |app| app.tier),
        file.closed_source.len()
    );

    write_yaml(
        OUTPUT_PATH,
        GeneratedApps {
            bar: PublishedBar {
                min_stars: threshold::MIN_STARS,
                activity_days: tier::threshold::ACTIVITY_DAYS,
            },
            apps: generated,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        "2026-09-01T00:00:00Z".parse().unwrap()
    }

    fn days_ago(days: i64) -> Option<DateTime<Utc>> {
        Some(now() - chrono::Duration::days(days))
    }

    fn shipping() -> Signals {
        Signals {
            on_crates_io: false,
            has_repo: true,
            repo_archived: Some(false),
            unreachable: Some(false),
            first_published: days_ago(900),
            last_real_commit: days_ago(3),
            last_push: days_ago(3),
            contributors: Some(200),
            stars: Some(30_000),
            has_description: Some(true),
            ..Signals::default()
        }
    }

    /// What a case is about, and the tweak to `shipping()` that gets it there.
    type Case = (&'static str, fn(&mut Signals));
    type Trigger = (ArchiveReason, fn(&mut Signals));

    fn but(tweak: impl FnOnce(&mut Signals)) -> Signals {
        let mut signals = shipping();
        tweak(&mut signals);
        signals
    }

    fn verdict_for(signals: &Signals) -> AppVerdict {
        decide(None, None, signals, now())
    }

    fn app(note: &str) -> InputApp {
        serde_yaml_ng::from_str(&format!(
            "name: Example\nrepository: https://github.com/example/example\n\
             topics: [applications]\nrust: pure-rust\nnote: {note:?}\n"
        ))
        .expect("a complete entry")
    }

    /// Nothing on this bar archives anything: a missing clause collapses the entry and does no
    /// more. The star case fails one short of the threshold, so tuning it cannot break this.
    #[test]
    fn a_failing_clause_names_itself_demotes_and_buries_nothing() {
        assert_eq!(verdict_for(&shipping()).tier, AppTier::Listed);

        let cases: [Case; 3] = [
            ("public_repo", |s| s.has_repo = false),
            ("stars", |s| s.stars = Some(threshold::MIN_STARS - 1)),
            ("recent_activity", |s| {
                s.last_real_commit = days_ago(400);
                s.last_push = days_ago(400);
            }),
        ];
        for (clause, break_it) in cases {
            let verdict = verdict_for(&but(break_it));
            assert_eq!(verdict.tier, AppTier::Watch, "{clause}");
            assert_eq!(verdict.bar.failed, vec![clause], "{clause}");
            assert_eq!(verdict.because, None, "{clause}");
            assert!(!verdict.auto_archived, "{clause}");
        }

        let exactly_on_it = but(|s| s.stars = Some(threshold::MIN_STARS));
        assert_eq!(verdict_for(&exactly_on_it).tier, AppTier::Listed);
    }

    #[test]
    fn an_unmeasured_clause_is_unknown_and_never_a_failure() {
        let cases: [Case; 2] = [
            ("stars", |s| s.stars = None),
            ("recent_activity", |s| s.last_real_commit = None),
        ];
        for (clause, unmeasure) in cases {
            let verdict = verdict_for(&but(unmeasure));
            assert_eq!(verdict.tier, AppTier::Listed, "{clause}");
            assert!(verdict.bar.failed.is_empty(), "{clause}");
            assert!(verdict.bar.unknown.contains(&clause), "{clause}");
        }
    }

    /// `tier.rs` tests the ladder itself; what this page needs is that it is reached at all.
    #[test]
    fn the_shared_archive_ladder_reaches_this_page_too() {
        let cases: [Trigger; 2] = [
            (ArchiveReason::RepoArchived, |s| {
                s.repo_archived = Some(true)
            }),
            (ArchiveReason::Dormant, |s| {
                s.last_real_commit = days_ago(1000);
                s.last_push = days_ago(1000);
            }),
        ];
        for (reason, fire) in cases {
            let verdict = verdict_for(&but(fire));
            assert_eq!(verdict.tier, AppTier::Archive, "{reason}");
            assert_eq!(verdict.because, Some(reason));
            assert!(
                verdict.auto_archived,
                "{reason}: nobody asserted this, so it has to un-archive itself"
            );
        }

        // A 404 on the only source this page has. `removed:` is the registry's word for a crate
        // that is gone, and there is no registry here, so the verdict is `unreachable` -- what the
        // catalog publishes for a repo-only entry in exactly this state.
        let gone = signals_for(&app("A product."), None, true);
        assert_eq!(gone.removed, None);
        let verdict = verdict_for(&gone);
        assert_eq!(verdict.because, Some(ArchiveReason::Unreachable));
        assert!(verdict.auto_archived);
    }

    #[test]
    fn an_archive_a_signal_can_detect_cannot_be_asserted_by_hand() {
        let input = InputApp {
            tier: Some(ManualAppTier::Archive),
            status_date: Some("2026-01-01".parse().unwrap()),
            because: Some(ArchiveReason::RepoArchived),
            evidence: Some("https://example.com/issue/1".parse().unwrap()),
            ..app("A product.")
        };
        let err = input.validate().unwrap_err().to_string();
        assert!(err.contains("detects"), "{err}");

        // The reasons no signal can reach are still assertable, and still override the bar.
        let superseded = InputApp {
            because: Some(ArchiveReason::Superseded),
            ..input
        };
        superseded.validate().unwrap();
        let verdict = decide(superseded.tier, superseded.because, &shipping(), now());
        assert_eq!(verdict.tier, AppTier::Archive);
        assert_eq!(verdict.because, Some(ArchiveReason::Superseded));
        assert!(!verdict.auto_archived);
    }

    #[test]
    fn an_entry_without_a_note_is_rejected() {
        let err = app("   ").validate().unwrap_err().to_string();
        assert!(err.contains("`note:`"), "{err}");
    }

    #[test]
    fn there_is_no_featured_status_to_write() {
        let err = serde_yaml_ng::from_str::<ManualAppTier>("featured").unwrap_err();
        assert!(err.to_string().contains("archive"), "{err}");
    }

    fn closed_source(name: &str, url: &str, note: &str) -> ClosedSource {
        ClosedSource {
            name: name.into(),
            url: url.parse().unwrap(),
            note: note.into(),
        }
    }

    #[test]
    fn a_closed_source_entry_needs_a_name_a_note_and_a_public_url() {
        let (name, url, note) = (
            "Infire",
            "https://blog.cloudflare.com/x",
            "Behind Workers AI.",
        );
        closed_source(name, url, note)
            .validate()
            .expect("a complete entry");

        for (case, name, url, note) in [
            ("no name", " ", url, note),
            ("no note", name, url, "  "),
            ("not a public url", name, "file:///etc/passwd", note),
        ] {
            assert!(closed_source(name, url, note).validate().is_err(), "{case}");
        }
    }

    #[test]
    fn the_sections_file_must_cover_every_section() {
        let one = vec![AppSection {
            id: AppTopic::Applications,
            title: "Applications".into(),
            catalog: Vec::new(),
        }];
        assert!(check_sections(&one).is_err());
    }
}
