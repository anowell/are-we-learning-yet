//! The curation model: four tiers, the published `listed` bar, and the archive triggers. The
//! invariant, which every clause here is shaped by: a missing signal never archives a live crate.
//! Triggers fire on positive evidence only, and a criterion that cannot be evaluated is unknown
//! rather than failed. Downloads and stars are deliberately not in the bar.

use crate::taxonomy::id_enum;
use chrono::{DateTime, Months, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

id_enum! {
    /// Where an entry sits on a hub page. Only `Featured` is written by a human.
    pub enum Tier {
        /// Demonstrated large impact, evidenced by somebody other than the crate's own authors.
        Featured = "featured",
        /// Meets the published bar. No personal endorsement.
        Listed = "listed",
        Watch = "watch",
        Archive = "archive",
    }
}

id_enum! {
    /// The part of `Tier` a human may assert in `_data/crates.yaml`.
    pub enum ManualTier {
        Featured = "featured",
        Archive = "archive",
    }
}

id_enum! {
    /// Why an entry is archived. A value outside this closed list fails the scrape.
    pub enum ArchiveReason {
        /// Somebody upstream said so. Inactivity alone is `Dormant`; complete is `finished: true`.
        Unmaintained = "unmaintained",
        /// No release and no human commit for `threshold::DORMANT_DAYS`. A fact, not an intent.
        Dormant = "dormant",
        RepoArchived = "repo_archived",
        /// The repo or the crate is gone; `Unreachable` is when neither resolves.
        RepoRemoved = "repo_removed",
        DeprecatedUpstream = "deprecated_upstream",
        /// Set `replacement:` alongside it.
        Superseded = "superseded",
        DoesNotBuild = "does_not_build",
        Unreachable = "unreachable",
        Yanked = "yanked",
        NoLongerMeetsCriteria = "no_longer_meets_criteria",
    }
}

impl ArchiveReason {
    /// A hand-written copy of a third-party fact keeps asserting itself after it stops being true,
    /// so `validate` rejects a manual archive claiming one of these; the computed one reverses.
    pub fn is_detected(self) -> bool {
        match self {
            ArchiveReason::Unmaintained
            | ArchiveReason::Dormant
            | ArchiveReason::RepoArchived
            | ArchiveReason::RepoRemoved
            | ArchiveReason::Unreachable
            | ArchiveReason::Yanked => true,
            ArchiveReason::DeprecatedUpstream
            | ArchiveReason::Superseded
            | ArchiveReason::DoesNotBuild
            | ArchiveReason::NoLongerMeetsCriteria => false,
        }
    }
}

id_enum! {
    /// What kind of independent evidence a `links:` entry is. `Docs` is not evidence (`validate`
    /// rejects it as the only one) but stays for `_data/apps.yaml`, where links are just reading.
    pub enum LinkKind {
        Docs = "docs",
        Talk = "talk",
        Post = "post",
        Benchmark = "benchmark",
        Production = "production",
        Paper = "paper",
    }
}

/// `deny_unknown_fields` does not recurse, so it is repeated on every nested type in this crate --
/// without it a stray key inside a `links:` element is silently dropped.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub kind: LinkKind,
    pub url: String,
    pub title: String,
}

/// The published bar's thresholds. The reasoning is published on `/about/`.
pub(crate) mod threshold {
    pub const FEATURED_LEASE_MONTHS: u32 = 18;
    pub const MIN_AGE_DAYS: i64 = 90;
    /// Widening it toward `DORMANT_DAYS` erases the distinction between "gone quiet" and "gone".
    pub const ACTIVITY_DAYS: i64 = 365;
    pub const DORMANT_DAYS: i64 = 730;
    /// Distinct *owners*: SciRS2's 224 reverse dependencies and RLX's 18 each resolve to one.
    pub const MIN_DISTINCT_OWNER_REVERSE_DEPS: u32 = 3;
    /// Three, not two: the counts include whoever started the project, so the intended "two
    /// contributors other than the author" is three commit authors.
    pub const MIN_CONTRIBUTORS: u32 = 3;
}

/// The measured inputs to a tier decision. Absent means *unknown*, never *no*. `Deserialize` reads
/// the previous run's generated file back, so a transient 500 cannot archive a live project.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Signals {
    /// Read from the input, never from a response: derived from whether crates.io answered, a
    /// timeout would fail a published crate's adoption clause instead of leaving it unknown.
    /// Whether the registry answered is `Measured::registry_consulted`.
    pub on_crates_io: bool,
    pub has_repo: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_archived: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yanked: Option<bool>,
    /// The crate 404s on crates.io, or a repo-only entry 404s on its forge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed: Option<bool>,
    /// Neither the crate nor the repository resolves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unreachable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustsec_unmaintained: Option<bool>,

    /// First publish, or the repository's creation for a repo-only entry. Drives the age gate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_published: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_published: Option<DateTime<Utc>>,
    /// Last commit by a human on the default branch. Deliberately *not* GitHub's `pushed_at`,
    /// which moves for bot pushes and PR branches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_real_commit: Option<DateTime<Utc>>,
    /// GitHub's `pushed_at`. Read only to veto an archive, never to prove liveness: its false
    /// positives all point at "looks alive and is not".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_push: Option<DateTime<Utc>>,

    /// A floor: owner lookups are budgeted (see `crates::OWNER_BUDGET`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distinct_owner_reverse_deps: Option<u32>,
    /// ecosyste.ms `dependent_repos_count`. A rendered fact, in neither the bar nor the ranking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependent_repos: Option<u32>,
    /// Distinct non-bot contributors; a floor on Codeberg and GitLab, which answer per commit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributors: Option<u32>,

    /// Un-deduplicated: rendered, never ranked on. The bar reads the count above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse_deps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stars: Option<u32>,

    /// `Some(false)` only once the override, crates.io and the repository have all been asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_description: Option<bool>,

    /// The human said this crate is complete rather than abandoned.
    pub finished: bool,
}

impl Signals {
    /// Fill what this run could not measure from the previous run, so a network blip cannot drop a
    /// crate below the bar. `registry`/`forge` gate it to sources actually consulted: a previous
    /// value stands in for a failed fetch, never for a source the entry no longer has -- otherwise
    /// a run that refuses a name collision carries the impostor's numbers forward forever.
    pub fn or_previous(&mut self, previous: &Signals, registry: bool, forge: bool) {
        macro_rules! carry {
            ($($field:ident),* $(,)?) => {
                $(if self.$field.is_none() { self.$field = previous.$field; })*
            };
        }
        if registry {
            carry!(
                yanked,
                removed,
                last_published,
                distinct_owner_reverse_deps,
                dependent_repos,
                reverse_deps,
                rustsec_unmaintained,
            );
        }
        if forge {
            carry!(
                repo_archived,
                last_real_commit,
                last_push,
                contributors,
                stars
            );
        }
        if registry || forge {
            // Either source can supply these.
            carry!(first_published, unreachable, has_description);
        }
    }
}

/// What a human asserted in `_data/crates.yaml`, reduced to what the decision needs.
#[derive(Clone, Debug, Default)]
pub struct Judgment {
    pub tier: Option<ManualTier>,
    pub status_date: Option<NaiveDate>,
    /// With `tier: archive` this is the assertion. Without one it archives nothing and only
    /// renames a trigger's reason, so a `repo_archived` entry can read "superseded, use X".
    pub because: Option<ArchiveReason>,
}

/// How each clause of the published bar came out. Unknown is not failure.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct BarResult {
    pub met: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unknown: Vec<&'static str>,
}

impl BarResult {
    /// The middle arm is the model: a criterion that could not be evaluated is unknown, never
    /// failed. Both bars on the site record through here, so there is one place to get it wrong.
    pub(crate) fn check(&mut self, name: &'static str, outcome: Option<bool>) {
        match outcome {
            Some(true) => {}
            Some(false) => self.failed.push(name),
            None => self.unknown.push(name),
        }
    }
}

/// Just enough of a previous run's generated entry to read its signals back. Both generated files
/// carry entries of this shape.
#[derive(Deserialize)]
pub struct PreviousEntry {
    pub id: String,
    pub signals: Signals,
}

impl PreviousEntry {
    pub fn by_id(entries: Vec<PreviousEntry>) -> HashMap<String, Signals> {
        entries
            .into_iter()
            .map(|entry| (entry.id, entry.signals))
            .collect()
    }
}

/// `3 featured, 40 listed, ...`: one line per run, so a scrape that suddenly archives half a page
/// is visible in CI. Generic because the catalog and the showcase have different tier vocabularies.
pub fn tally<T: PartialEq + std::fmt::Display, E>(
    all: &[T],
    entries: &[E],
    tier_of: impl Fn(&E) -> T,
) -> String {
    let count = |tier: &T| entries.iter().filter(|e| tier_of(e) == *tier).count();
    all.iter()
        .map(|tier| format!("{} {tier}", count(tier)))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    pub tier: Tier,
    /// The human's `because:` when there is one, otherwise the trigger's own reason.
    pub because: Option<ArchiveReason>,
    /// Computed rather than asserted, so the entry un-archives when the trigger clears.
    pub auto_archived: bool,
    /// A `featured` entry past its lease; the page says so rather than demoting it.
    pub stale_since: Option<NaiveDate>,
    pub bar: BarResult,
}

pub(crate) fn older_than(
    when: Option<DateTime<Utc>>,
    days: i64,
    now: DateTime<Utc>,
) -> Option<bool> {
    when.map(|when| (now - when).num_days() > days)
}

/// Three-valued OR: "no release in a year" plus "nobody read the commit log" is unknown, not no.
pub(crate) fn any_of<const N: usize>(routes: [Option<bool>; N]) -> Option<bool> {
    if routes.contains(&Some(true)) {
        Some(true)
    } else if routes.contains(&None) {
        None
    } else {
        Some(false)
    }
}

/// Third-party facts that outrank everyone, including a human's `featured`.
pub(crate) fn hard_trigger(signals: &Signals) -> Option<ArchiveReason> {
    if signals.repo_archived == Some(true) {
        return Some(ArchiveReason::RepoArchived);
    }
    if signals.removed == Some(true) {
        return Some(ArchiveReason::RepoRemoved);
    }
    if signals.unreachable == Some(true) {
        return Some(ArchiveReason::Unreachable);
    }
    if signals.rustsec_unmaintained == Some(true) {
        return Some(ArchiveReason::Unmaintained);
    }
    if signals.yanked == Some(true) {
        return Some(ArchiveReason::Yanked);
    }
    None
}

/// Never fires without a known last-real-commit date, or every crate whose history failed to fetch
/// archives itself on the strength of a stale release date alone.
pub(crate) fn dormant(signals: &Signals, now: DateTime<Utc>) -> bool {
    if signals.finished {
        return false;
    }
    let days = threshold::DORMANT_DAYS;
    let commit_stale = older_than(signals.last_real_commit, days, now) == Some(true);
    let publish_stale = older_than(signals.last_published, days, now) != Some(false);
    let push_stale = older_than(signals.last_push, days, now) != Some(false);
    commit_stale && publish_stale && push_stale
}

fn evaluate_bar(signals: &Signals, now: DateTime<Utc>) -> BarResult {
    let mut bar = BarResult::default();

    bar.check("published", Some(signals.on_crates_io || signals.has_repo));

    for (name, flag) in [
        ("repo_not_archived", signals.repo_archived),
        ("not_yanked", signals.yanked),
        ("not_rustsec_unmaintained", signals.rustsec_unmaintained),
    ] {
        bar.check(name, flag.map(|flagged| !flagged));
    }

    bar.check(
        "age",
        older_than(signals.first_published, threshold::MIN_AGE_DAYS, now),
    );

    // Inapplicable, not unevaluable: a repo-only entry has no registry it could have released to,
    // and left unknown the clause could never fail however dead the repo was.
    let days = threshold::ACTIVITY_DAYS;
    let released_recently = if signals.on_crates_io {
        older_than(signals.last_published, days, now).map(|stale| !stale)
    } else {
        Some(false)
    };
    let activity = any_of([
        Some(signals.finished),
        older_than(signals.last_real_commit, days, now).map(|stale| !stale),
        released_recently,
    ]);
    bar.check("recent_activity", activity);

    // Same rule: unknown here would let a URL and a one-line PR reach `listed` on day 91.
    let reverse_deps = if signals.on_crates_io {
        signals
            .distinct_owner_reverse_deps
            .map(|n| n >= threshold::MIN_DISTINCT_OWNER_REVERSE_DEPS)
    } else {
        Some(false)
    };
    let adoption = any_of([
        reverse_deps,
        signals
            .contributors
            .map(|n| n >= threshold::MIN_CONTRIBUTORS),
    ]);
    bar.check("adoption", adoption);

    // Deliberately not an archive trigger: a description says nothing about whether it is alive.
    bar.check("described", signals.has_description);

    bar.met = bar.failed.is_empty();
    bar
}

/// The archive ladder, shared by the catalog and the showcase so the two pages cannot disagree
/// about what a fact means. `dormancy_applies` is false for an entry a curator vouched for -- a
/// named human beats two silent years -- but the hard triggers outrank even that.
pub(crate) fn archived_because(
    manual_archive: bool,
    because: Option<ArchiveReason>,
    dormancy_applies: bool,
    signals: &Signals,
    now: DateTime<Utc>,
) -> Option<(Option<ArchiveReason>, bool)> {
    if let Some(reason) = hard_trigger(signals) {
        return Some((because.or(Some(reason)), !manual_archive));
    }
    if manual_archive {
        return Some((because, false));
    }
    if dormancy_applies && dormant(signals, now) {
        return Some((because.or(Some(ArchiveReason::Dormant)), true));
    }
    None
}

/// An expired lease keeps the entry in `featured` with a dated mark rather than demoting it,
/// because demotion would drop the prose that makes the staleness legible.
pub fn decide(judgment: &Judgment, signals: &Signals, now: DateTime<Utc>) -> Verdict {
    let bar = evaluate_bar(signals, now);
    let manual_archive = judgment.tier == Some(ManualTier::Archive);
    let manual_featured = judgment.tier == Some(ManualTier::Featured);

    if let Some((because, auto_archived)) = archived_because(
        manual_archive,
        judgment.because,
        !manual_featured,
        signals,
        now,
    ) {
        return Verdict {
            tier: Tier::Archive,
            because,
            auto_archived,
            stale_since: None,
            bar,
        };
    }

    if manual_featured {
        return Verdict {
            tier: Tier::Featured,
            because: None,
            auto_archived: false,
            stale_since: judgment.status_date.filter(|date| {
                date.checked_add_months(Months::new(threshold::FEATURED_LEASE_MONTHS))
                    .is_some_and(|expiry| expiry < now.date_naive())
            }),
            bar,
        };
    }

    let tier = if bar.met { Tier::Listed } else { Tier::Watch };
    Verdict {
        tier,
        because: None,
        auto_archived: false,
        stale_since: None,
        bar,
    }
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

    fn healthy() -> Signals {
        Signals {
            on_crates_io: true,
            has_repo: true,
            repo_archived: Some(false),
            yanked: Some(false),
            rustsec_unmaintained: Some(false),
            first_published: days_ago(2000),
            last_published: days_ago(30),
            last_real_commit: days_ago(10),
            last_push: days_ago(10),
            distinct_owner_reverse_deps: Some(9),
            dependent_repos: Some(40),
            contributors: Some(12),
            has_description: Some(true),
            ..Signals::default()
        }
    }

    /// The clause a case is about, and the tweak to `healthy()` that gets it there.
    type Case = (&'static str, fn(&mut Signals));

    /// A healthy crate with one thing wrong: every case below is the diff against the baseline.
    fn but(tweak: impl FnOnce(&mut Signals)) -> Signals {
        let mut signals = healthy();
        tweak(&mut signals);
        signals
    }

    /// Silent on every date the dormancy trigger reads.
    fn silent() -> Signals {
        but(|s| {
            s.last_published = days_ago(1500);
            s.last_push = days_ago(1500);
            s.last_real_commit = days_ago(1500);
        })
    }

    fn verdict(judgment: &Judgment, signals: &Signals) -> Verdict {
        decide(judgment, signals, now())
    }

    fn tier(signals: &Signals) -> Tier {
        verdict(&Judgment::default(), signals).tier
    }

    fn featured(status_date: &str) -> Judgment {
        Judgment {
            tier: Some(ManualTier::Featured),
            status_date: Some(status_date.parse().unwrap()),
            because: None,
        }
    }

    fn annotated(reason: ArchiveReason) -> Judgment {
        Judgment {
            tier: None,
            status_date: Some("2026-09-02".parse().unwrap()),
            because: Some(reason),
        }
    }

    #[test]
    fn a_failing_clause_names_itself_demotes_and_buries_nothing() {
        assert_eq!(tier(&healthy()), Tier::Listed, "the baseline clears it");

        let cases: [Case; 5] = [
            ("published", |s| {
                s.on_crates_io = false;
                s.has_repo = false;
            }),
            ("age", |s| s.first_published = days_ago(20)),
            ("recent_activity", |s| {
                s.last_published = days_ago(500);
                s.last_real_commit = days_ago(500);
                s.last_push = days_ago(500);
            }),
            ("adoption", |s| {
                s.distinct_owner_reverse_deps = Some(0);
                s.contributors = Some(1);
                // Lockfile depth is a rendered fact, not a route: 6975 of them do not rescue it.
                s.dependent_repos = Some(6975);
            }),
            ("described", |s| s.has_description = Some(false)),
        ];

        for (clause, break_it) in cases {
            let verdict = verdict(&Judgment::default(), &but(break_it));
            assert_eq!(verdict.tier, Tier::Watch, "{clause}");
            assert_eq!(verdict.bar.failed, vec![clause]);
            assert_eq!(verdict.because, None, "{clause}: the bar buries nothing");
            assert!(!verdict.auto_archived, "{clause}");
        }
    }

    /// The invariant the file rests on, so an outage leaves every entry where it was.
    #[test]
    fn an_unmeasured_clause_is_unknown_and_never_a_failure() {
        let cases: [Case; 4] = [
            ("adoption", |s| {
                s.distinct_owner_reverse_deps = None;
                s.contributors = None;
            }),
            // An unreadable forge, or a branch whose recent commits are all a bot's.
            ("recent_activity", |s| {
                s.on_crates_io = false;
                s.last_published = None;
                s.last_real_commit = None;
                s.last_push = days_ago(500);
            }),
            ("described", |s| s.has_description = None),
            // Unauthenticated GitLab nulls `archived`, and "we cannot see the flag" is not "the
            // owner has not set it".
            ("repo_not_archived", |s| s.repo_archived = None),
        ];

        for (clause, unmeasure) in cases {
            let verdict = verdict(&Judgment::default(), &but(unmeasure));
            assert_eq!(verdict.tier, Tier::Listed, "{clause}");
            assert!(verdict.bar.failed.is_empty(), "{clause}: {:?}", verdict.bar);
            assert!(verdict.bar.unknown.contains(&clause), "{:?}", verdict.bar);
        }
    }

    #[test]
    fn an_entry_with_nothing_known_is_listed_and_never_archived() {
        let signals = Signals {
            has_repo: true,
            ..Signals::default()
        };
        let verdict = verdict(&Judgment::default(), &signals);
        assert_eq!(verdict.tier, Tier::Listed);
        assert!(verdict.bar.failed.is_empty());
        assert!(!verdict.bar.unknown.is_empty());
    }

    #[test]
    fn the_registry_routes_are_inapplicable_to_a_repo_only_entry_not_unknown() {
        let released_but_unread = but(|s| s.last_real_commit = None);
        let published = verdict(&Judgment::default(), &released_but_unread);
        assert_eq!(published.tier, Tier::Listed);
        assert!(published.bar.failed.is_empty());
        assert!(!published.bar.unknown.contains(&"recent_activity"));

        let cold_repo = but(|s| {
            s.on_crates_io = false;
            s.last_published = None;
            s.last_real_commit = days_ago(500);
            s.last_push = days_ago(500);
            s.contributors = Some(20);
        });
        let cold = verdict(&Judgment::default(), &cold_repo);
        assert_eq!(cold.tier, Tier::Watch);
        assert_eq!(cold.bar.failed, vec!["recent_activity"]);

        let solo_repo = but(|s| {
            s.on_crates_io = false;
            s.distinct_owner_reverse_deps = None;
            s.contributors = Some(1);
        });
        let solo = verdict(&Judgment::default(), &solo_repo);
        assert_eq!(solo.tier, Tier::Watch);
        assert_eq!(solo.bar.failed, vec!["adoption"]);
    }

    #[test]
    fn contributors_alone_carry_adoption_from_the_threshold_up() {
        let short = but(|s| {
            s.distinct_owner_reverse_deps = Some(0);
            s.dependent_repos = None;
            s.contributors = Some(threshold::MIN_CONTRIBUTORS - 1);
        });
        assert_eq!(tier(&short), Tier::Watch);

        let at_the_threshold = Signals {
            contributors: Some(threshold::MIN_CONTRIBUTORS),
            ..short
        };
        assert_eq!(tier(&at_the_threshold), Tier::Listed);
    }

    #[test]
    fn dormancy_needs_a_known_commit_date_and_three_things_veto_it() {
        let confirmed = verdict(&Judgment::default(), &silent());
        assert_eq!(confirmed.tier, Tier::Archive);
        assert!(confirmed.auto_archived);

        let mut unread = silent();
        unread.last_real_commit = None;
        assert_eq!(tier(&unread), Tier::Listed);

        let mut pushed = silent();
        pushed.last_push = days_ago(30);
        assert_eq!(tier(&pushed), Tier::Watch);

        let mut done = silent();
        done.finished = true;
        assert_eq!(tier(&done), Tier::Listed);
    }

    #[test]
    fn dormancy_and_an_advisory_are_not_the_same_word() {
        // `dormant` reads a commit log; `unmaintained` needs somebody to have said it.
        let quiet = verdict(&Judgment::default(), &silent());
        assert_eq!(quiet.because, Some(ArchiveReason::Dormant));

        let mut advised = silent();
        advised.rustsec_unmaintained = Some(true);
        let advised = verdict(&Judgment::default(), &advised);
        assert_eq!(advised.because, Some(ArchiveReason::Unmaintained));
    }

    #[test]
    fn a_failed_fetch_carries_forward_and_a_source_that_is_gone_does_not() {
        let yesterday = healthy();

        let mut today = but(|s| {
            s.distinct_owner_reverse_deps = None;
            s.dependent_repos = None;
            s.last_real_commit = None;
        });
        today.or_previous(&yesterday, true, true);
        assert_eq!(today, yesterday);

        let mut refused = but(|s| {
            s.on_crates_io = false;
            s.distinct_owner_reverse_deps = None;
            s.last_published = None;
            s.yanked = None;
        });
        refused.or_previous(&yesterday, false, true);
        assert_eq!(refused.distinct_owner_reverse_deps, None);
        assert_eq!(refused.last_published, None);
        assert_eq!(refused.yanked, None);
        // The repository is still a source, so what it measured still stands in.
        assert_eq!(refused.last_real_commit, yesterday.last_real_commit);
    }

    #[test]
    fn an_unreadable_forge_does_not_manufacture_a_missing_description() {
        // Yesterday all three sources answered and none had a description; today the forge 500s.
        // The carry that stops a blip archiving a crate also stops one promoting it.
        let yesterday = but(|s| s.has_description = Some(false));
        let mut today = but(|s| s.has_description = None);
        today.or_previous(&yesterday, true, true);
        assert_eq!(today.has_description, Some(false));
        assert_eq!(tier(&today), Tier::Watch);
    }

    #[test]
    fn the_lease_expires_a_day_after_its_term() {
        let granted = now()
            .date_naive()
            .checked_sub_months(Months::new(threshold::FEATURED_LEASE_MONTHS))
            .expect("the lease term is representable");
        assert_eq!(
            verdict(&featured(&granted.to_string()), &healthy()).stale_since,
            None,
            "the last day of the lease is not yet stale"
        );

        let lapsed = granted.pred_opt().unwrap();
        let verdict = verdict(&featured(&lapsed.to_string()), &healthy());
        assert_eq!(verdict.tier, Tier::Featured, "a lapse does not demote");
        assert_eq!(verdict.stale_since, Some(lapsed));
    }

    #[test]
    fn a_third_party_flag_outranks_the_curator_and_the_curator_outranks_dormancy() {
        let vouched = featured("2026-08-01");
        assert_eq!(verdict(&vouched, &silent()).tier, Tier::Featured);

        let mut yanked = silent();
        yanked.yanked = Some(true);
        let yank = verdict(&vouched, &yanked);
        assert_eq!(yank.tier, Tier::Archive);
        assert_eq!(yank.because, Some(ArchiveReason::Yanked));

        let signals = but(|s| s.repo_archived = Some(true));
        for judgment in [
            Judgment::default(),
            vouched,
            featured("2020-01-01"),
            Judgment {
                tier: Some(ManualTier::Archive),
                status_date: Some("2026-08-31".parse().unwrap()),
                because: Some(ArchiveReason::Superseded),
            },
        ] {
            assert_eq!(
                verdict(&judgment, &signals).tier,
                Tier::Archive,
                "an owner archiving their own repository outranks everyone, curator included"
            );
        }

        let bar = verdict(&Judgment::default(), &signals).bar;
        assert!(bar.failed.contains(&"repo_not_archived"));
        assert!(!bar.met);
        assert_eq!(tier(&but(|s| s.repo_archived = Some(false))), Tier::Listed);
    }

    #[test]
    fn the_detected_reasons_are_exactly_the_ones_a_trigger_emits() {
        let triggers: [fn(&mut Signals); 5] = [
            |s| s.repo_archived = Some(true),
            |s| s.removed = Some(true),
            |s| s.unreachable = Some(true),
            |s| s.rustsec_unmaintained = Some(true),
            |s| s.yanked = Some(true),
        ];
        let mut emitted: Vec<Option<ArchiveReason>> =
            triggers.map(|fire| hard_trigger(&but(fire))).into();
        emitted.push(verdict(&Judgment::default(), &silent()).because);

        for reason in emitted.iter().flatten() {
            assert!(
                reason.is_detected(),
                "{reason} is produced by a trigger, so a human asserting it adds nothing"
            );
        }
        for reason in ArchiveReason::ALL {
            if reason.is_detected() {
                assert!(
                    emitted.contains(&Some(*reason)),
                    "{reason} is marked detected but no trigger emits it, so rejecting it from \
                     `_data/crates.yaml` would leave it unsayable by anyone"
                );
            }
        }
    }

    #[test]
    fn an_annotated_reason_renames_an_automatic_archive_and_causes_none() {
        let annotation = annotated(ArchiveReason::Superseded);
        assert_eq!(verdict(&annotation, &healthy()).tier, Tier::Listed);
        assert_eq!(
            verdict(&annotation, &Signals::default()).tier,
            Tier::Watch,
            "an annotation must not archive an entry the scraper knows nothing about"
        );

        for signals in [but(|s| s.repo_archived = Some(true)), silent()] {
            let archived = verdict(&annotation, &signals);
            assert_eq!(archived.tier, Tier::Archive);
            assert_eq!(archived.because, Some(ArchiveReason::Superseded));
            assert!(
                archived.auto_archived,
                "nobody asserted this tier, so the page must say it un-archives itself"
            );
        }
    }

    #[test]
    fn a_hand_written_archive_still_outranks_a_bar_it_passes() {
        // What the manual path survives for: a live monorepo whose Rust directory was deleted
        // reads as healthy on every signal the scraper can see.
        let judgment = Judgment {
            tier: Some(ManualTier::Archive),
            status_date: Some("2026-09-01".parse().unwrap()),
            because: Some(ArchiveReason::DeprecatedUpstream),
        };
        let verdict = verdict(&judgment, &healthy());
        assert_eq!(verdict.tier, Tier::Archive);
        assert_eq!(verdict.because, Some(ArchiveReason::DeprecatedUpstream));
        assert!(!verdict.auto_archived);
        assert!(
            verdict.bar.met,
            "the bar is unaffected; the human overrode it"
        );
    }
}
