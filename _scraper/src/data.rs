use crate::collision;
use crate::crates::CrateData;
use crate::forge::RepoData;
use crate::score;
use crate::taxonomy::Topic;
use crate::tier::{ArchiveReason, BarResult, Judgment, Link, LinkKind, ManualTier, Signals, Tier};
use anyhow::{Result, bail};
use chrono::{DateTime, NaiveDate, Utc};
use crates_io_api::Crate;
use serde::{Deserialize, Serialize};
use url::Url;

/// One entry as a human wrote it in `_data/crates.yaml`: judgment and corrections only, so a diff
/// of that file is a diff of opinions. Unknown keys are rejected -- a mistyped `evidnece:` would
/// otherwise be dropped in silence.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct InputCrateInfo {
    pub name: Option<String>,
    pub topics: Vec<Topic>,

    // --- the human's judgment (see `crate::tier`) ---
    /// Only ever `featured`, or `archive` for one of the reasons no signal can produce.
    pub tier: Option<ManualTier>,
    /// When the judgment above was made. Drives the `featured` lease and dates an archive.
    pub status_date: Option<NaiveDate>,
    /// Why this entry is archived. Without `tier: archive` it is an annotation that renames an
    /// automatic archive and never causes one.
    pub because: Option<ArchiveReason>,
    /// A public, citable URL for `because:`. Usually the issue where upstream was asked first.
    pub evidence: Option<Url>,
    /// What to use instead of this entry, once it is archived.
    pub replacement: Option<String>,
    pub verified_by: Option<String>,
    /// What they actually looked at.
    pub verified_version: Option<String>,
    /// This crate is complete, not abandoned. Opts out of the inactivity trigger.
    #[serde(default)]
    pub finished: bool,
    /// Featured only: what it is and where it fits.
    pub featured_note: Option<String>,
    /// Featured only: independent evidence. At least one has to be something other than
    /// `kind: docs`.
    #[serde(default)]
    pub links: Vec<Link>,

    // --- overrides for what the registries report ---
    pub documentation: Option<String>,
    pub repository: Option<Url>,
    pub license: Option<String>,
    pub description: Option<String>,
}

impl InputCrateInfo {
    /// How to name this entry in an error message.
    pub fn id(&self) -> String {
        self.name
            .clone()
            .or_else(|| self.repository.as_ref().map(Url::to_string))
            .unwrap_or_else(|| format!("{self:?}"))
    }

    pub fn judgment(&self) -> Judgment {
        Judgment {
            tier: self.tier,
            status_date: self.status_date,
            because: self.because,
        }
    }

    /// Rejects a judgment that is not self-supporting: a claim about someone else's project has
    /// to carry the thing that makes it checkable, or it fails the build.
    pub fn validate(&self) -> Result<()> {
        let id = self.id();

        if self.topics.is_empty() {
            bail!("{id} lists no topics, so it would not appear on any page");
        }

        if let Some(name) = &self.name
            && collision::is_collision(name)
            && self.repository.is_none()
        {
            bail!(
                "{id} is a known crate-name collision, so it must pin `repository:` saying which \
                 project it means. crates.io holds something else under that name"
            );
        }

        if let Some(date) = self.status_date
            && date > Utc::now().date_naive()
        {
            bail!("{id} has a status_date in the future ({date})");
        }

        // Every URL here is rendered as an `href`, and `url::Url` deserializes `javascript:` as
        // happily as `https:`.
        if let Some(url) = &self.repository
            && !matches!(url.scheme(), "http" | "https")
        {
            bail!("{id} has a `repository:` that is not a public URL ({url})");
        }
        if let Some(url) = &self.documentation
            && !url.starts_with("http://")
            && !url.starts_with("https://")
        {
            bail!("{id} has a `documentation:` that is not a public URL ({url})");
        }

        if let Some(url) = &self.evidence
            && !matches!(url.scheme(), "http" | "https")
        {
            bail!("{id} has `evidence:` that is not a public URL ({url})");
        }
        if self.evidence.is_some() && self.because.is_none() {
            bail!("{id} cites `evidence:` with no `because:` for it to be evidence of");
        }

        // Naming one as an *annotation*, without `tier:`, is still allowed.
        if let Some(reason) = self.because
            && self.tier == Some(ManualTier::Archive)
            && reason.is_detected()
        {
            bail!(
                "{id} asserts `tier: archive` with `because: {reason}`, which the scraper detects \
                 for itself on every run. Drop the `tier:` line -- the trigger archives it, and \
                 un-archives it if the fact changes. Keep `because:`, `evidence:` and \
                 `replacement:` if they say more than the trigger does. If the flag is wrong \
                 because the project MOVED, the fix is a corrected `repository:` pointing at where \
                 development actually happens, not an archive"
            );
        }

        match self.tier {
            Some(ManualTier::Archive) => {
                if self.because.is_none() {
                    bail!(
                        "{id} is archived without `because:`. Pick one the scraper cannot detect \
                         for itself: {}",
                        ArchiveReason::ALL
                            .iter()
                            .filter(|reason| !reason.is_detected())
                            .map(|reason| reason.id())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                if self.evidence.is_none() {
                    bail!(
                        "{id} is archived without `evidence:`. Ask upstream in a public issue \
                         first, then cite it here"
                    );
                }
                if self.status_date.is_none() {
                    bail!("{id} is archived without `status_date:` saying when that was decided");
                }
            }
            Some(ManualTier::Featured) => {
                if self
                    .featured_note
                    .as_ref()
                    .is_none_or(|s| s.trim().is_empty())
                {
                    bail!(
                        "{id} is marked featured without `featured_note:` prose saying what it \
                         is and where it fits"
                    );
                }
                if self.links.is_empty() {
                    bail!(
                        "{id} is marked featured with no `links:`. Featured means demonstrated \
                         large impact, and the evidence for that has to come from somebody else \
                         -- a talk, a post, a benchmark, a production user"
                    );
                }
                if self.links.iter().all(|link| link.kind == LinkKind::Docs) {
                    bail!(
                        "{id} is marked featured on `kind: docs` links only, which is not \
                         evidence: a crate's own documentation says nothing about whether it moved \
                         anybody, and `documentation:` already links it. At least one link has to \
                         come from somebody else -- a talk, a post, a benchmark, a production user"
                    );
                }
                if self.status_date.is_none() {
                    bail!(
                        "{id} is marked featured without `status_date:`, so its re-verification \
                         clock would never start"
                    );
                }
            }
            None => {}
        }

        if self.finished {
            if self.verified_by.is_none() {
                bail!(
                    "{id} sets `finished: true` without `verified_by:`. It opts the entry out \
                     of the dormancy trigger permanently, so it needs a name on it"
                );
            }
            if self.status_date.is_none() {
                bail!(
                    "{id} sets `finished: true` without `status_date:` saying when that was \
                     decided, so the quarterly review has nothing to re-check"
                );
            }
        }

        // `because`, `evidence` and `replacement` are deliberately not in this list: the entries
        // that need them most are the ones a trigger archived, which carry no `tier:` at all.
        for (field, present, needs) in [
            (
                "featured_note",
                self.featured_note.is_some(),
                ManualTier::Featured,
            ),
            ("links", !self.links.is_empty(), ManualTier::Featured),
        ] {
            if present && self.tier != Some(needs) {
                bail!("{id} sets `{field}:`, which only applies to `tier: {needs}`");
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

#[derive(Serialize, Clone, Debug)]
pub struct GeneratedCrateInfo {
    /// The crate name, or the repository URL for a repo-only entry. Read back by the next run to
    /// match an entry to its previous signals.
    pub id: String,

    pub topics: Vec<Topic>,

    /// Computed on every scrape, so an archive un-archives itself the moment its trigger clears.
    pub tier: Tier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub because: Option<ArchiveReason>,
    pub auto_archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_since: Option<NaiveDate>,

    // The human's judgment, carried through verbatim for rendering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub featured_note: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    pub finished: bool,

    /// Written out so a disagreement with a verdict is a disagreement about a number.
    pub signals: Signals,
    pub bar: BarResult,

    pub score: Option<u64>,

    /// The entry pins a `repository:` that disagrees with the one crates.io holds. Usually a
    /// correction -- projects move -- but it is also the shape a wrong attachment takes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_mismatch: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// Resolved once here rather than three times in Liquid, and top-level because a repo-only
    /// entry has no `meta` to hang it on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Resolved like `description`; for a published crate it is `meta.documentation`, docs.rs
    /// fallback included.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,

    #[serde(rename = "meta", skip_serializing_if = "Option::is_none")]
    pub krate: Option<Crate>,

    #[serde(rename = "repo", skip_serializing_if = "Option::is_none")]
    pub repo: Option<RepoData>,
}

impl From<&InputCrateInfo> for GeneratedCrateInfo {
    fn from(input: &InputCrateInfo) -> Self {
        GeneratedCrateInfo {
            id: input.id(),
            topics: input.topics.clone(),
            // Recomputed by `finish`; `watch` is the safe starting point.
            tier: Tier::Watch,
            because: None,
            auto_archived: false,
            stale_since: None,
            status_date: input.status_date,
            evidence: input.evidence.as_ref().map(Url::to_string),
            replacement: input.replacement.clone(),
            verified_by: input.verified_by.clone(),
            verified_version: input.verified_version.clone(),
            featured_note: input.featured_note.clone(),
            links: input.links.clone(),
            finished: input.finished,
            signals: Signals::default(),
            bar: BarResult::default(),
            score: None,
            repo_mismatch: None,
            license: None,
            description: None,
            documentation: None,
            krate: None,
            repo: None,
        }
    }
}

fn registry_repo(data: &CrateData) -> Option<Url> {
    data.registry_repository
        .as_deref()
        .and_then(|url| Url::parse(url).ok())
}

fn replace_opt<T: Clone>(original: &mut Option<T>, extra: &Option<T>) {
    if let Some(val) = extra {
        let _ = original.replace(val.clone());
    }
}

impl GeneratedCrateInfo {
    /// Attach what crates.io holds under this entry's name unless it demonstrably is not this
    /// project, and say whether it was attached. A refusal leaves the entry rendering from its
    /// repository alone.
    pub fn attach_crate_data(&mut self, input: &InputCrateInfo, data: CrateData) -> bool {
        let disagreement = match (&input.repository, registry_repo(&data)) {
            (Some(pin), Some(registry)) if !collision::same_repo(pin, &registry) => Some(registry),
            _ => None,
        };

        if let Some(registry) = disagreement {
            let name = data.krate.name.clone();
            if collision::is_collision(&name) {
                eprintln!(
                    "  ! not attaching crates.io/{name}: it points at {registry}, and this entry \
                     pins {}. Known name collision -- rendering from the repository only",
                    input.repository.as_ref().expect("pin checked above")
                );
                return false;
            }
            // Usually a move and a `repository:` correction that says so; printed in case it is
            // not.
            println!("  ~ {name}: crates.io says {registry}, entry pins the repository instead");
            self.repo_mismatch = Some(registry.to_string());
        }

        self.license = data.license;
        self.krate = Some(data.krate);
        true
    }

    pub fn apply_overrides(&mut self, input: &InputCrateInfo) {
        replace_opt(&mut self.license, &input.license);
        // Seeded before the early return below, so a hand-written value survives an entry with no
        // `krate`. The docs.rs fallback stays on the `krate` branch: no crate, no docs.rs page.
        replace_opt(&mut self.description, &input.description);
        replace_opt(&mut self.documentation, &input.documentation);

        let Some(krate) = self.krate.as_mut() else {
            return;
        };
        replace_opt(&mut krate.documentation, &input.documentation);
        if krate.documentation.is_none() {
            krate.documentation = Some(format!("https://docs.rs/crate/{}", krate.name));
        }
        self.documentation = krate.documentation.clone();
        replace_opt(
            &mut krate.repository,
            &input.repository.as_ref().map(Url::to_string),
        );
        replace_opt(&mut krate.description, &input.description);
    }

    pub fn repository(&self, input: &InputCrateInfo) -> Option<Url> {
        self.krate
            .as_ref()
            .and_then(|k| k.repository.as_deref())
            .and_then(|r| Url::parse(r).ok())
            .or_else(|| input.repository.clone())
    }

    /// Override, then crates.io, then the repository. The repository comes last because a crate's
    /// blurb describes the artifact a reader would install, while the repository's describes a
    /// whole project, often a workspace of several.
    fn describe(&self, input: &InputCrateInfo) -> Option<String> {
        fn prose(text: &str) -> Option<String> {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_string())
        }
        input
            .description
            .as_deref()
            .and_then(prose)
            .or_else(|| {
                self.krate
                    .as_ref()
                    .and_then(|krate| krate.description.as_deref())
                    .and_then(prose)
            })
            .or_else(|| {
                self.repo
                    .as_ref()
                    .and_then(|repo| repo.description.as_deref())
                    .and_then(prose)
            })
    }

    /// `Some(false)` claims *nobody wrote one*, so it is returned only once the registry and the
    /// forge have both genuinely answered -- or have nothing to answer for.
    fn has_description(&self, input: &InputCrateInfo, measured: &Measured) -> Option<bool> {
        if self.description.is_some() {
            return Some(true);
        }
        let registry_answered =
            input.name.is_none() || self.krate.is_some() || measured.removed == Some(true);
        let forge_answered = self.repository(input).is_none() || self.repo.is_some();
        (registry_answered && forge_answered).then_some(false)
    }

    fn signals(&self, input: &InputCrateInfo, measured: &Measured) -> Signals {
        let repo = self.repo.as_ref();
        Signals {
            // The entry's claim, not this run's answer -- see `Signals::on_crates_io`.
            on_crates_io: measured.registry_consulted,
            has_repo: self.repository(input).is_some(),
            repo_archived: repo.and_then(|r| r.archived),
            yanked: measured.latest_yanked,
            removed: measured.removed,
            unreachable: measured.unreachable,
            rustsec_unmaintained: measured.rustsec_unmaintained,
            // Without the repository fallback the age gate is unevaluable for exactly the entries
            // that most obviously pass it.
            first_published: self
                .krate
                .as_ref()
                .map(|k| k.created_at)
                .or_else(|| repo.and_then(|r| r.created_at)),
            last_published: self.krate.as_ref().map(|k| k.updated_at),
            last_real_commit: repo.and_then(|r| r.last_real_commit),
            last_push: repo.and_then(|r| r.last_commit),
            distinct_owner_reverse_deps: measured.distinct_owner_reverse_deps,
            dependent_repos: measured.dependent_repos,
            contributors: repo.and_then(|r| r.contributors),
            reverse_deps: measured.reverse_deps,
            stars: repo.map(|r| r.stargazers_count),
            has_description: self.has_description(input, measured),
            finished: input.finished,
        }
    }

    /// Compute the signal set, the tier and the score. `previous` is this entry's signals from the
    /// last `crates_generated.yaml`.
    pub fn finish(
        &mut self,
        input: &InputCrateInfo,
        measured: &Measured,
        previous: Option<&Signals>,
        now: DateTime<Utc>,
    ) {
        // Before the signals, because one of them is whether this resolved to anything.
        self.description = self.describe(input);

        let mut signals = self.signals(input, measured);
        if let Some(previous) = previous {
            signals.or_previous(
                previous,
                measured.registry_consulted,
                measured.forge_consulted,
            );
        }

        let verdict = crate::tier::decide(&input.judgment(), &signals, now);
        self.tier = verdict.tier;
        self.because = verdict.because;
        self.auto_archived = verdict.auto_archived;
        self.stale_since = verdict.stale_since;
        self.bar = verdict.bar;
        self.score = Some(score::score(&signals, now));
        self.signals = signals;
    }
}

/// What the fetchers found, for the fields not carried on `krate` or `repo`. `None` means nobody
/// could answer.
#[derive(Default, Debug)]
pub struct Measured {
    /// crates.io was asked about a crate that belongs to *this* entry. False when the entry names
    /// no crate, and false when the name resolved to somebody else's project and was refused.
    pub registry_consulted: bool,
    /// A repository host the scraper can read was asked about this entry.
    pub forge_consulted: bool,
    pub latest_yanked: Option<bool>,
    pub removed: Option<bool>,
    pub unreachable: Option<bool>,
    pub rustsec_unmaintained: Option<bool>,
    pub distinct_owner_reverse_deps: Option<u32>,
    pub dependent_repos: Option<u32>,
    pub reverse_deps: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::RepoData;

    /// A repo-only entry: no `name`, so no crates.io and no `meta`.
    fn repo_only(description: Option<&str>) -> InputCrateInfo {
        InputCrateInfo {
            topics: vec![Topic::LlmServing],
            repository: Some(
                "https://github.com/example/thing"
                    .parse()
                    .expect("valid URL"),
            ),
            description: description.map(str::to_string),
            ..InputCrateInfo::default()
        }
    }

    fn forge_data(description: Option<&str>) -> RepoData {
        RepoData {
            name: "example/thing".into(),
            url: "https://github.com/example/thing".into(),
            host: "github.com".into(),
            description: description.map(str::to_string),
            stargazers_count: 3,
            last_commit: Some(Utc::now()),
            last_real_commit: None,
            contributors: None,
            created_at: None,
            archived: Some(false),
        }
    }

    /// The real pipeline order: overrides before the forge is asked, `finish` afterwards.
    fn generate(input: &InputCrateInfo, repo: Option<RepoData>) -> GeneratedCrateInfo {
        let mut entry = GeneratedCrateInfo::from(input);
        entry.apply_overrides(input);
        entry.repo = repo;
        entry.finish(input, &Measured::default(), None, Utc::now());
        entry
    }

    fn named(name: &str) -> InputCrateInfo {
        InputCrateInfo {
            name: Some(name.to_string()),
            repository: None,
            ..repo_only(Some("A thing."))
        }
    }

    fn evidence() -> Option<Url> {
        Some(Url::parse("https://github.com/example/thing/issues/1").expect("valid URL"))
    }

    fn archived(reason: ArchiveReason) -> InputCrateInfo {
        InputCrateInfo {
            tier: Some(ManualTier::Archive),
            because: Some(reason),
            evidence: evidence(),
            status_date: Some("2026-09-01".parse().expect("valid date")),
            ..named("thing")
        }
    }

    /// Over the whole vocabulary, so a reason added to either side of `is_detected` is covered.
    #[test]
    fn a_hand_written_archive_for_a_detected_reason_is_rejected() {
        for reason in ArchiveReason::ALL {
            let result = archived(*reason).validate();
            if !reason.is_detected() {
                result.unwrap_or_else(|err| panic!("{reason} has no automatic route: {err}"));
                continue;
            }
            let err = result
                .expect_err("a detected reason is not a curator's to assert")
                .to_string();
            assert!(
                err.contains("detects") && err.contains("repository:"),
                "the error has to say what to do instead, both for a dead crate and for a moved \
                 one: {err}"
            );
        }
    }

    #[test]
    fn an_annotation_survives_without_a_manual_tier_even_for_a_detected_reason() {
        let input = InputCrateInfo {
            because: Some(ArchiveReason::Superseded),
            evidence: evidence(),
            replacement: Some("sherpa-onnx".into()),
            status_date: Some("2026-09-02".parse().expect("valid date")),
            ..named("sherpa-rs")
        };
        input.validate().expect("an annotation is not an assertion");
        assert_eq!(input.judgment().because, Some(ArchiveReason::Superseded));
        assert_eq!(input.judgment().tier, None);

        InputCrateInfo {
            because: Some(ArchiveReason::RepoArchived),
            ..input
        }
        .validate()
        .expect("naming the fact is not asserting the tier");
    }

    #[test]
    fn a_manual_archive_needs_a_reason_evidence_and_a_date_and_nothing_dangling() {
        let complete = archived(ArchiveReason::Superseded);
        complete.validate().expect("a complete archive");

        let mut no_evidence = complete.clone();
        no_evidence.evidence = None;
        let accusation = "an archive without evidence is an accusation";
        assert!(no_evidence.validate().is_err(), "{accusation}");

        let mut no_date = complete.clone();
        no_date.status_date = None;
        assert!(no_date.validate().is_err(), "an archive needs its date");

        let dangling = InputCrateInfo {
            evidence: evidence(),
            ..named("thing")
        };
        assert!(dangling.validate().is_err(), "evidence for nothing");

        // No `evidence:` either, or it trips the rule about evidence for nothing first.
        let no_reason = InputCrateInfo {
            because: None,
            evidence: None,
            ..complete
        };
        let err = no_reason.validate().expect_err("no reason").to_string();
        assert!(err.contains("superseded"));
        assert!(
            !err.contains("repo_archived"),
            "offering a reason the schema then rejects is a trap: {err}"
        );
    }

    fn link(kind: LinkKind) -> Link {
        Link {
            kind,
            url: "https://example.com/thing".into(),
            title: "Somebody else wrote this".into(),
        }
    }

    fn featured(links: Vec<Link>) -> InputCrateInfo {
        InputCrateInfo {
            tier: Some(ManualTier::Featured),
            featured_note: Some("What it is and where it fits.".into()),
            status_date: Some("2026-09-01".parse().expect("valid date")),
            links,
            ..named("thing")
        }
    }

    /// No links and docs-only links get their own messages: the two want different fixes.
    #[test]
    fn featuring_needs_one_link_that_is_not_the_crates_own_documentation() {
        let none = featured(Vec::new())
            .validate()
            .expect_err("featured needs evidence")
            .to_string();
        assert!(none.contains("no `links:`"), "{none}");

        let err = featured(vec![link(LinkKind::Docs), link(LinkKind::Docs)])
            .validate()
            .expect_err("a crate's own documentation is not somebody else's evidence")
            .to_string();
        assert!(
            err.contains("docs") && err.contains("documentation:"),
            "the error has to say why a docs link does not count and where docs do belong: {err}"
        );

        // A docs link alongside real evidence is fine; it is only not evidence by itself.
        for kind in LinkKind::ALL.iter().filter(|kind| **kind != LinkKind::Docs) {
            featured(vec![link(LinkKind::Docs), link(*kind)])
                .validate()
                .unwrap_or_else(|err| panic!("{kind} comes from a third party: {err}"));
        }
    }

    /// The override is seeded before `apply_overrides` returns early on a missing `krate`, so it
    /// survives an entry that has no crates.io page to hang a docs.rs fallback off.
    #[test]
    fn a_hand_written_documentation_url_reaches_a_repo_only_entry() {
        let docs = "https://pengowen123.github.io/eant2/eant2/index.html";
        let input = InputCrateInfo {
            documentation: Some(docs.into()),
            ..repo_only(None)
        };
        let entry = generate(&input, Some(forge_data(None)));
        assert_eq!(entry.documentation.as_deref(), Some(docs));
    }

    /// Both fields are rendered as an `href`, and `url::Url` parses `javascript:` as happily as
    /// `https:`.
    #[test]
    fn a_repository_or_documentation_that_is_not_a_public_url_fails_the_scrape() {
        let hostile = "javascript:alert(1)";
        let err = InputCrateInfo {
            repository: Some(Url::parse(hostile).expect("parses as a URL")),
            ..named("thing")
        }
        .validate()
        .expect_err("a javascript: repository is not a link")
        .to_string();
        assert!(err.contains("repository"), "{err}");

        let docs = InputCrateInfo {
            documentation: Some(hostile.into()),
            ..named("thing")
        };
        assert!(docs.validate().is_err());
        repo_only(Some("A thing."))
            .validate()
            .expect("http(s) is fine");
    }

    #[test]
    fn a_crates_io_outage_does_not_turn_a_published_entry_into_a_repo_only_one() {
        // Read off `krate`, the bar's inapplicability branches would report a failure for an
        // outage.
        let input = InputCrateInfo {
            name: Some("gemm".into()),
            ..repo_only(None)
        };
        let measured = Measured {
            registry_consulted: true,
            ..Measured::default()
        };
        let mut entry = GeneratedCrateInfo::from(&input);
        entry.apply_overrides(&input);
        entry.repo = Some(forge_data(Some("Fast matrix multiplication")));
        entry.finish(&input, &measured, None, Utc::now());
        assert!(
            entry.signals.on_crates_io,
            "the entry names a crate; whether crates.io answered is a different question"
        );
        assert!(entry.bar.unknown.contains(&"adoption"));
        assert!(!entry.bar.failed.contains(&"adoption"));

        let repo_only = generate(&repo_only(None), Some(forge_data(None)));
        assert!(!repo_only.signals.on_crates_io);
    }

    #[test]
    fn a_forge_that_gave_no_push_date_leaves_it_unknown() {
        // Defaulting this to `Utc::now()` makes the entry permanently immune to the dormancy
        // trigger, because `push_stale` cannot be true of a date that is always today.
        let input = repo_only(None);
        let undated = RepoData {
            last_commit: None,
            ..forge_data(None)
        };
        let entry = generate(&input, Some(undated));
        assert_eq!(entry.signals.last_push, None);
    }

    /// Override, then the forge, and `Some(false)` only once the forge has actually answered: a
    /// 500 must leave the clause unknown rather than demoting the entry for a description nobody
    /// looked for.
    #[test]
    fn a_description_resolves_in_order_and_says_when_nobody_wrote_one() {
        let mine = "What it actually is.";
        let theirs = "Rust bindings for libfoo";
        // (what the human wrote, what the forge said, what the page shows, what the bar knows).
        // The forge column is doubly optional: `None` is a 500, `Some(None)` an answer with none.
        for (over, forge, shown, known) in [
            (Some(mine), Some(Some(theirs)), Some(mine), Some(true)),
            (None, Some(Some(theirs)), Some(theirs), Some(true)),
            (None, Some(None), None, Some(false)),
            (None, None, None, None),
        ] {
            let case = format!("override {over:?}, forge {forge:?}");
            let entry = generate(&repo_only(over), forge.map(forge_data));
            assert_eq!(entry.description.as_deref(), shown, "{case}");
            assert_eq!(entry.signals.has_description, known, "{case}");
            let (failed, unknown) = (&entry.bar.failed, &entry.bar.unknown);
            assert_eq!(
                failed.contains(&"described"),
                known == Some(false),
                "{case}"
            );
            assert_eq!(unknown.contains(&"described"), known.is_none(), "{case}");
        }
    }
}
