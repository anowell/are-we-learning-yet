//! Ranking within a tier. Deliberately absent: downloads, because crates.io counts are
//! un-deduplicated CDN logs (`plotters` carries a 48M badge that is really criterion's), and
//! ecosyste.ms `dependent_repos_count`/`dependent_packages_count`, which read lockfile depth and
//! un-deduplicated registry packages rather than use. Downloads are shown on the page as a fact;
//! they order nothing.

use crate::tier::Signals;
use chrono::{DateTime, Utc};

mod weight {
    pub const REVERSE_DEPS: f64 = 65.0;
    pub const CONTRIBUTORS_INSTEAD: f64 = REVERSE_DEPS;
    pub const RECENCY: f64 = 20.0;
    pub const AGE: f64 = 15.0;
}

/// Deliberately low: a hub page answers "which of these is load-bearing", not "rank the top 1%".
mod saturate {
    /// High enough that the top of a mature shelf is not decided by the stars tie-breaker.
    pub const REVERSE_DEPS: f64 = 50.0;
    pub const CONTRIBUTORS: f64 = 100.0;
    pub const STARS: f64 = 100_000.0;
    pub const AGE_YEARS: f64 = 5.0;
}

fn log_norm(value: u32, saturation: f64) -> f64 {
    (f64::from(value) + 1.0).ln() / (saturation + 1.0).ln()
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn recency(signals: &Signals, now: DateTime<Utc>) -> f64 {
    if signals.finished {
        return 1.0;
    }
    let Some(last) = signals.last_real_commit else {
        // Unknown is not imputed: this is what a known-stale date scores too.
        return 0.0;
    };
    match (now - last).num_days() {
        ..=180 => 1.0,
        181..=365 => 0.5,
        366..=730 => 0.25,
        _ => 0.0,
    }
}

/// Span, not age: a project that shipped for six years and stopped keeps this bonus and loses the
/// recency term instead.
fn age_bonus(signals: &Signals) -> f64 {
    let Some(first) = signals.first_published else {
        return 0.0;
    };
    let last = [signals.last_real_commit, signals.last_published]
        .into_iter()
        .flatten()
        .max();
    let Some(last) = last else {
        return 0.0;
    };
    let years = (last - first).num_days() as f64 / 365.25;
    clamp01(years / saturate::AGE_YEARS)
}

fn adoption(signals: &Signals) -> f64 {
    // A repo-only entry has no registry to be depended on in, so it is measured by the one route
    // it has.
    if !signals.on_crates_io {
        return match signals.contributors {
            Some(count) => {
                weight::CONTRIBUTORS_INSTEAD * clamp01(log_norm(count, saturate::CONTRIBUTORS))
            }
            None => 0.0,
        };
    }
    let reverse_deps = signals
        .distinct_owner_reverse_deps
        .map_or(0.0, |count| log_norm(count, saturate::REVERSE_DEPS));
    weight::REVERSE_DEPS * clamp01(reverse_deps)
}

/// Larger sorts first. The tie-breaker is capped at 999, so stars can only separate entries the
/// real terms scored identically.
pub fn score(signals: &Signals, now: DateTime<Utc>) -> u64 {
    let weighted = adoption(signals)
        + weight::RECENCY * recency(signals, now)
        + weight::AGE * age_bonus(signals);

    let primary = (weighted * 1000.0).round().max(0.0) as u64;
    let tiebreak = signals
        .stars
        .map_or(0.0, |stars| {
            999.0 * clamp01(log_norm(stars, saturate::STARS))
        })
        .round() as u64;
    primary * 1000 + tiebreak
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

    fn hub_crate() -> Signals {
        Signals {
            on_crates_io: true,
            has_repo: true,
            first_published: days_ago(2500),
            last_published: days_ago(20),
            last_real_commit: days_ago(5),
            distinct_owner_reverse_deps: Some(25),
            stars: Some(4000),
            ..Signals::default()
        }
    }

    /// The hub crate with one thing changed, scored.
    fn scored(tweak: impl FnOnce(&mut Signals)) -> u64 {
        let mut signals = hub_crate();
        tweak(&mut signals);
        score(&signals, now())
    }

    /// Every assertion here is an ordering, never a number: the weights are meant to be tuned, and
    /// a test that pins one would fail on a tuning rather than on a defect.
    #[test]
    fn reverse_deps_outweigh_recency_which_outweighs_age() {
        // `last_published` is held at today so the age bonus is identical in every variant and
        // each comparison is one term against one term.
        let base = |s: &mut Signals| {
            s.distinct_owner_reverse_deps = Some(0);
            s.last_real_commit = days_ago(900);
            s.first_published = days_ago(900);
            s.last_published = days_ago(0);
            s.stars = None;
        };
        let one_term = |tweak: fn(&mut Signals)| {
            scored(|s| {
                base(s);
                tweak(s);
            })
        };
        let rdeps = one_term(|s| s.distinct_owner_reverse_deps = Some(50));
        let recent = one_term(|s| s.last_real_commit = days_ago(10));
        let sustained = one_term(|s| s.first_published = days_ago(4000));

        assert!(rdeps > recent, "reverse deps are the heaviest term");
        assert!(recent > sustained, "recency outweighs age");
        assert!(sustained > scored(base));
    }

    /// The two counts the ranking is deliberately blind to. Downloads are not on `Signals` at all;
    /// `dependent_repos` is, and is rendered but never ranked on.
    #[test]
    fn dependent_repos_are_not_an_input() {
        assert_eq!(
            scored(|s| s.dependent_repos = Some(6975)),
            scored(|s| s.dependent_repos = Some(1))
        );
    }

    #[test]
    fn stars_only_break_ties() {
        let modest = |s: &mut Signals| {
            s.distinct_owner_reverse_deps = Some(6);
            s.stars = Some(0);
        };
        let famous = scored(|s| {
            modest(s);
            s.stars = Some(90_000);
        });
        assert!(famous > scored(modest), "stars separate an exact tie");
        assert!(
            scored(|s| {
                modest(s);
                s.distinct_owner_reverse_deps = Some(7);
            }) > famous,
            "and one more dependent outranks 90k stars"
        );
    }

    /// Unknown scores what a known-bad value scores. Imputing anything else would rank an outage
    /// above a measurement.
    #[test]
    fn an_unknown_signal_neither_credits_nor_penalizes_beyond_itself() {
        assert_eq!(
            scored(|s| {
                s.distinct_owner_reverse_deps = None;
                s.last_real_commit = None;
            }),
            scored(|s| {
                s.distinct_owner_reverse_deps = Some(0);
                s.last_real_commit = days_ago(3000);
            })
        );
    }

    /// The registry route reads zero for an entry that is not on the registry, so without the
    /// contributors branch every repo-only entry would rank on recency and age alone.
    #[test]
    fn a_repo_only_entry_is_ranked_on_the_signal_it_has() {
        let repo_only = |s: &mut Signals| s.on_crates_io = false;
        assert!(
            scored(|s| {
                repo_only(s);
                s.contributors = Some(80);
            }) > scored(repo_only)
        );
    }

    #[test]
    fn finished_crates_keep_their_recency() {
        let stale = |s: &mut Signals| s.last_real_commit = days_ago(1500);
        assert!(
            scored(|s| {
                stale(s);
                s.finished = true;
            }) > scored(stale)
        );
    }
}
