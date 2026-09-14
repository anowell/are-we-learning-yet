//! The curation nag, written to the build's step summary. A `featured` lease and a hand-written
//! archive are both claims that only stay true if somebody re-reads them. It reads the generated
//! catalog rather than re-deriving anything, so it reports exactly what the site is rendering.

use crate::tier::threshold::FEATURED_LEASE_MONTHS;
use crate::tier::{ArchiveReason, Tier};
use anyhow::Result;
use chrono::{Months, NaiveDate, Utc};
use serde::Deserialize;

/// How far ahead to warn, so an endorsement is raised before the page starts saying it is stale.
const WARN_AHEAD_MONTHS: u32 = 3;

/// Deliberately a permissive view of `GeneratedCrateInfo`: this runs against a file some older
/// invocation of the scraper wrote.
#[derive(Deserialize, Default)]
#[serde(default)]
struct Entry {
    id: String,
    tier: Option<Tier>,
    because: Option<ArchiveReason>,
    auto_archived: bool,
    stale_since: Option<NaiveDate>,
    status_date: Option<NaiveDate>,
    verified_by: Option<String>,
    finished: bool,
}

fn lapses_on(status_date: NaiveDate) -> Option<NaiveDate> {
    status_date.checked_add_months(Months::new(FEATURED_LEASE_MONTHS))
}

fn cell(value: Option<impl ToString>) -> String {
    value.map_or_else(|| "-".into(), |value| value.to_string())
}

fn section(out: &mut String, heading: &str, columns: &[&str], rows: Vec<Vec<String>>, why: &str) {
    out.push_str(&format!("## {heading}\n\n"));
    if rows.is_empty() {
        out.push_str("None.\n\n");
        return;
    }
    out.push_str(&format!(
        "| {} |\n|{}\n",
        columns.join(" | "),
        "---|".repeat(columns.len())
    ));
    for row in rows {
        out.push_str(&format!("| {} |\n", row.join(" | ")));
    }
    out.push_str(&format!("\n{why}\n\n"));
}

pub fn render(path: &str) -> Result<String> {
    let entries: Vec<Entry> = crate::util::read_yaml(path)?;
    let today = Utc::now().date_naive();
    let warn_before = today.checked_add_months(Months::new(WARN_AHEAD_MONTHS));

    let mut asserted_archives: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.tier == Some(Tier::Archive) && !e.auto_archived)
        .collect();
    asserted_archives.sort_by(|a, b| a.id.cmp(&b.id));

    let mut expiring: Vec<&Entry> = entries
        .iter()
        .filter(|entry| entry.tier == Some(Tier::Featured))
        .filter(|entry| {
            entry.stale_since.is_some()
                || matches!(
                    (entry.status_date.and_then(lapses_on), warn_before),
                    (Some(lapse), Some(warn)) if lapse <= warn
                )
        })
        .collect();
    expiring.sort_by_key(|entry| entry.status_date);

    let mut out = String::from("# Curation review\n\nNothing here has been changed for you.\n\n");
    section(
        &mut out,
        "`featured` endorsements to re-affirm or let lapse",
        &["Entry", "Judged", "Lease ends", "Signed by", "State"],
        expiring
            .iter()
            .map(|e| {
                vec![
                    format!("`{}`", e.id),
                    cell(e.status_date),
                    cell(e.status_date.and_then(lapses_on)),
                    cell(e.verified_by.as_deref()),
                    if e.stale_since.is_some() {
                        "**lapsed** -- the page already says so".into()
                    } else {
                        "lapses before the next review".into()
                    },
                ]
            })
            .collect(),
        "Re-affirm by bumping `status_date:` after actually re-reading the project; let it lapse by \
         dropping `tier: featured` and its prose.",
    );

    section(
        &mut out,
        "Archives still asserted by hand",
        &["Entry", "Reason", "Judged", "Signed by"],
        asserted_archives
            .iter()
            .map(|e| {
                vec![
                    format!("`{}`", e.id),
                    e.because.map_or("-", ArchiveReason::id).to_string(),
                    cell(e.status_date),
                    cell(e.verified_by.as_deref()),
                ]
            })
            .collect(),
        "No signal will ever clear one of these, so the question is whether the reason still holds.",
    );

    section(
        &mut out,
        "Entries marked `finished: true`",
        &["Entry", "Judged", "Signed by"],
        entries
            .iter()
            .filter(|e| e.finished)
            .map(|e| {
                vec![
                    format!("`{}`", e.id),
                    cell(e.status_date),
                    cell(e.verified_by.as_deref()),
                ]
            })
            .collect(),
        "The flag exempts these from the dormancy trigger, so no archive can ever reach them. \
         Complete software still stops compiling; if one is dead rather than done, drop the flag.",
    );

    Ok(out)
}
