//! Every inline reference in the site's prose resolves, or the scrape fails. Four checks on one
//! idiom -- `assign` + `include` -- covering crate links (`xref`), citations beside a claim
//! (`cite_*`), superscripts with the source below the block (`ref_*`), and the `[n]` markers in
//! `_data/topics.yaml`, which are the same receipt as data because a YAML scalar cannot include.
//!
//! Here rather than in the templates, because Liquid cannot fail a build. No url is fetched: link
//! rot is a different problem, and a build should not fail on somebody else's outage.

use anyhow::{Result, bail};
use chrono::{NaiveDate, Utc};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use url::Url;

use crate::taxonomy::HubDef;

/// Directories with nothing a reader ever sees. `_scraper` is here because it quotes the syntax.
const SKIP_DIRS: &[&str] = &[
    "_site",
    "_tmp",
    "_scraper",
    "_data",
    "node_modules",
    ".git",
    ".jj",
    ".runes",
    ".toren",
    ".github",
];

/// Named here as well as in [`CITE`] because the error prose quotes it back at the author.
const CITE_TEMPLATE: &str = "cite.liquid";

const REF_TEMPLATE: &str = "ref.liquid";

/// What a `sources:` entry in `_data/topics.yaml` calls the same three fields.
const BLURB_FIELDS: &[&str] = &["src", "url", "date"];

/// The one layout that pulls carried entries back out of `page.content`. Under any other layout a
/// reference renders its number and silently prints no source: the fence is an HTML comment.
const REF_LAYOUT: &str = "crates.liquid";

/// Reserved: `ref.liquid` fences each carried entry with it and `crates.liquid` splits on it, so
/// an author writing it in prose splits the page somewhere the layout does not expect.
const REF_SENTINEL: &str = "<!--ref-->";

/// The mechanism, not a call site: it assigns `xref` in its own usage example.
const XREF_TEMPLATE: &str = "cratecard.liquid";

/// Hosts where a citation without a date is always wrong. `cite_date` stays optional elsewhere
/// because a living document has no date, so the requirement is drawn where it is mechanical.
const DATED_BY_CONSTRUCTION: &[&str] = &["reddit.com", "users.rust-lang.org"];

/// The `"..."` or `'...'` at the front of `rest`. Liquid string literals have no escapes, so the
/// first matching quote ends it.
fn quoted(rest: &str) -> Option<&str> {
    let rest = rest.trim_start();
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let value = &rest[quote.len_utf8()..];
    Some(&value[..value.find(quote)?])
}

/// Every `<name> = "..."` assignment in one file, with its byte offset. Requiring `=` right after
/// the name is what stops the templates' own `xref_found` matching.
fn assignments(text: &str, name: &str) -> Vec<(usize, String)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;

    while let Some(hit) = text[at..].find(name) {
        let start = at + hit;
        at = start + name.len();

        // A word merely *ending* in the name: `..._xref`.
        if start > 0 && matches!(bytes[start - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') {
            continue;
        }
        let Some(rest) = text[at..].trim_start().strip_prefix('=') else {
            continue;
        };
        if let Some(value) = quoted(rest) {
            found.push((start, value.to_string()));
        }
    }

    found
}

/// The `xref` values in one file, in source order.
fn scan(text: &str) -> Vec<String> {
    assignments(text, "xref")
        .into_iter()
        .map(|(_, v)| v)
        .collect()
}

/// Byte offset of each `{% include "<template>" %}` tag, so prose naming the template is not one.
fn includes_of(text: &str, template: &str) -> Vec<usize> {
    let mut found = Vec::new();
    let mut at = 0;

    while let Some(hit) = text[at..].find("include") {
        let start = at + hit;
        at = start + "include".len();

        if quoted(&text[at..]).is_some_and(|value| value.trim() == template) {
            found.push(start);
        }
    }

    found
}

/// The three slots every vocabulary lists first, in [`check_source`]'s order. A fourth (`ref_note`)
/// has no rule and is scanned only so a note-only call site with no include is an orphan.
const SRC: usize = 0;
const URL: usize = 1;
const DATE: usize = 2;

/// One include, and the fields standing when it was reached. `assign` is global and unscoped, so
/// the template sees the most recent assignment; both templates clear on the way out, so `None`
/// here means what it means on the page -- empty, not inherited.
#[derive(Debug, PartialEq, Eq)]
struct Marker {
    line: usize,
    /// Aligned with the names the scan was given.
    values: Vec<Option<String>>,
    /// Assignments with no include to consume them: a citation that renders as nothing at all.
    orphan: bool,
}

/// Group one file's `<prefix>_*` assignments into one marker per include of `template`.
fn scan_markers(text: &str, names: &[&str], template: &str) -> Vec<Marker> {
    let mut events: Vec<(usize, Option<(usize, String)>)> = Vec::new();
    for (slot, name) in names.iter().enumerate() {
        for (pos, value) in assignments(text, name) {
            events.push((pos, Some((slot, value))));
        }
    }
    for pos in includes_of(text, template) {
        events.push((pos, None));
    }
    events.sort_by_key(|(pos, _)| *pos);

    let mut out = Vec::new();
    let mut pending: Vec<Option<String>> = vec![None; names.len()];
    let mut first_pending: Option<usize> = None;

    for (pos, event) in events {
        match event {
            Some((slot, value)) => {
                if first_pending.is_none() {
                    first_pending = Some(pos);
                }
                pending[slot] = Some(value);
            }
            None => {
                out.push(Marker {
                    line: line_of(text, pos),
                    values: std::mem::replace(&mut pending, vec![None; names.len()]),
                    orphan: false,
                });
                first_pending = None;
            }
        }
    }

    // A page may open by setting the fields to "", so only a non-empty leftover is a lost marker.
    let leftover_has_content = pending
        .iter()
        .any(|v| v.as_deref().is_some_and(|v| !v.trim().is_empty()));
    if leftover_has_content {
        out.push(Marker {
            line: first_pending.map_or(1, |pos| line_of(text, pos)),
            values: pending,
            orphan: true,
        });
    }

    out
}

/// A page's front matter, if it opens with one. A file without one is a fragment, and nothing here
/// has an opinion about the layout it is rendered under.
fn front_matter(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("---")?;
    rest.find("\n---").map(|end| &rest[..end])
}

/// The `layout:` that front matter names.
fn layout_of(text: &str) -> Option<&str> {
    front_matter(text)?
        .lines()
        .find_map(|l| l.trim().strip_prefix("layout:"))
        .map(str::trim)
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].bytes().filter(|b| *b == b'\n').count() + 1
}

/// Hand every Liquid-rendered file under `root` to `visit`, as (path, text).
fn walk(dir: &Path, visit: &mut dyn FnMut(&Path, &str) -> Result<()>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if !SKIP_DIRS.contains(&name) {
                walk(&path, visit)?;
            }
            continue;
        }
        // The two things cobalt renders Liquid in.
        if !matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("md") | Some("liquid")
        ) {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        visit(&path, &text)?;
    }
    Ok(())
}

/// Find every crate reference under `root` and fail if any of them names nothing in the catalog.
pub fn check(root: &Path, ids: &HashSet<String>) -> Result<()> {
    let mut broken: Vec<String> = Vec::new();
    let mut found = 0usize;
    let mut files: HashSet<PathBuf> = HashSet::new();
    walk(root, &mut |path, text| {
        if path.file_name().and_then(|n| n.to_str()) == Some(XREF_TEMPLATE) {
            return Ok(());
        }
        for id in scan(text) {
            found += 1;
            files.insert(path.to_path_buf());
            if !ids.contains(&id) {
                broken.push(format!("  {} references `{id}`", path.display()));
            }
        }
        Ok(())
    })?;

    if !broken.is_empty() {
        bail!(
            "{} crate reference(s) name nothing in _data/crates.yaml:\n{}\n\
             An `xref` is an entry's `id` there -- the crate name, or the repository URL for a \
             repo-only entry. Either the entry was renamed or removed, or the reference is a typo. \
             Fixing the prose is the usual answer; adding the crate is the other one.",
            broken.len(),
            broken.join("\n")
        );
    }

    println!(
        "Crate references: {found} in {} file(s), all resolve",
        files.len()
    );
    Ok(())
}

/// The rules a source obeys wherever it is written. `names` is what each vocabulary calls the
/// fields, so the message names the thing the author has to go and fix.
fn check_source(
    at: &str,
    names: &[&str],
    src: Option<&str>,
    url: Option<&str>,
    date: Option<&str>,
    today: NaiveDate,
    problems: &mut Vec<String>,
) {
    let (n_src, n_url, n_date) = (names[0], names[1], names[2]);

    match src.map(str::trim) {
        None => problems.push(format!("{at} has no `{n_src}`")),
        Some("") => problems.push(format!("{at} has an empty `{n_src}`")),
        Some(_) => {}
    }

    let date = date.map(str::trim).filter(|d| !d.is_empty());

    match url.map(str::trim) {
        None => problems.push(format!("{at} has no `{n_url}`")),
        Some("") => problems.push(format!("{at} has an empty `{n_url}`")),
        Some(raw) => match Url::parse(raw) {
            Err(e) => problems.push(format!(
                "{at} has `{n_url}` `{raw}`, which is not a url ({e})"
            )),
            Ok(url) if !matches!(url.scheme(), "http" | "https") => problems.push(format!(
                "{at} has `{n_url}` `{raw}`, whose scheme is `{}` and not http(s)",
                url.scheme()
            )),
            Ok(url) => {
                let host = url.host_str().unwrap_or_default();
                let dated = DATED_BY_CONSTRUCTION
                    .iter()
                    .any(|h| host == *h || host.ends_with(&format!(".{h}")));
                if dated && date.is_none() {
                    problems.push(format!(
                        "{at} cites `{host}` with no `{n_date}`, and a forum post is dated by \
                         construction"
                    ));
                }
            }
        },
    }

    if let Some(d) = date {
        match d.parse::<NaiveDate>() {
            Err(_) => problems.push(format!(
                "{at} has `{n_date}` `{d}`, which is not a YYYY-MM-DD date"
            )),
            Ok(parsed) if parsed > today => {
                problems.push(format!("{at} has `{n_date}` `{d}`, which is in the future"))
            }
            Ok(_) => {}
        }
    }
}

/// One marker vocabulary. The two on the site differ only in field names, template and which files
/// are mechanism rather than call site; the rules are [`check_source`]'s, written once.
struct MarkerKind {
    fields: &'static [&'static str],
    template: &'static str,
    /// Files that carry the idiom without being a call site, skipped by name.
    mechanism: &'static [&'static str],
}

/// Beside the claim. `cite.liquid` names all three variables in its own documentation.
const CITE: MarkerKind = MarkerKind {
    fields: &["cite_src", "cite_url", "cite_date"],
    template: CITE_TEMPLATE,
    mechanism: &[CITE_TEMPLATE],
};

/// In a superscript, source printed below the text block. Neither skipped file is a call site:
/// `ref.liquid` documents its own variables, and `crates.liquid` drives it from `sources:` data, so
/// a scan of its text sees an include with no assignments and would report every shelf on the site.
const REF: MarkerKind = MarkerKind {
    fields: &["ref_src", "ref_url", "ref_date", "ref_note"],
    template: REF_TEMPLATE,
    mechanism: &[REF_TEMPLATE, REF_LAYOUT],
};

impl MarkerKind {
    /// Every marker of this vocabulary under `root`. `per_file` adds the rules only one
    /// vocabulary has.
    fn collect(
        &self,
        root: &Path,
        problems: &mut Vec<String>,
        mut per_file: impl FnMut(&Path, &str, &[Marker], &mut Vec<String>),
    ) -> Result<(Vec<Marker>, usize)> {
        let today = Utc::now().date_naive();
        let mut found: Vec<Marker> = Vec::new();
        let mut files: HashSet<PathBuf> = HashSet::new();

        walk(root, &mut |path, text| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if self.mechanism.contains(&name) {
                return Ok(());
            }
            let markers = scan_markers(text, self.fields, self.template);
            per_file(path, text, &markers, problems);

            for marker in markers {
                let at = format!("  {}:{}", path.display(), marker.line);
                if marker.orphan {
                    problems.push(format!(
                        "{at} assigns `{}_*` with no `{{% include \"{}\" %}}` after it",
                        // The field prefix is the template's own name, in both vocabularies.
                        self.template.trim_end_matches(".liquid"),
                        self.template
                    ));
                    continue;
                }
                files.insert(path.to_path_buf());
                check_source(
                    &at,
                    self.fields,
                    marker.values[SRC].as_deref(),
                    marker.values[URL].as_deref(),
                    marker.values[DATE].as_deref(),
                    today,
                    problems,
                );
                found.push(marker);
            }
            Ok(())
        })?;

        Ok((found, files.len()))
    }
}

/// Find every source citation under `root` and fail if any of them is malformed.
pub fn check_citations(root: &Path) -> Result<()> {
    let mut problems: Vec<String> = Vec::new();
    let (cites, files) = CITE.collect(root, &mut problems, |_, _, _, _| {})?;

    if !problems.is_empty() {
        bail!(
            "{} source citation(s) in the site's prose are malformed:\n{}\n\
             A citation is `{{% assign cite_src %}}` + `{{% assign cite_url %}}` + \
             `{{% include \"{}\" %}}`; `cite_date` is optional except on a forum thread. The \
             marker names its source in visible text, so an empty field is a claim on the page with \
             nowhere to check it. Urls are not fetched here -- this is shape, not reachability.",
            problems.len(),
            problems.join("\n"),
            CITE_TEMPLATE
        );
    }

    let dated = cites.iter().filter(|m| m.values[DATE].is_some()).count();
    println!(
        "Source citations: {} in {files} file(s), all well-formed ({dated} dated)",
        cites.len()
    );

    // Same idiom, same walk: one traversal, and one call for a caller to remember.
    check_refs(root)
}

/// Same rules as [`check_citations`] over the same machinery. The extra two are this vocabulary's
/// alone: its source rides out of the page inside the rendered HTML, so it needs the one layout
/// that unpacks it and the fence that layout splits on left alone.
pub fn check_refs(root: &Path) -> Result<()> {
    let mut problems: Vec<String> = Vec::new();
    let (refs, files) = REF.collect(root, &mut problems, |path, text, markers, problems| {
        if let Some(pos) = text.find(REF_SENTINEL) {
            problems.push(format!(
                "  {}:{} writes `{REF_SENTINEL}` in its own prose, which is the fence the layout \
                 splits the page on",
                path.display(),
                line_of(text, pos)
            ));
        }
        if !markers.is_empty()
            && front_matter(text).is_some()
            && layout_of(text) != Some(REF_LAYOUT)
        {
            problems.push(format!(
                "  {}:1 uses `{REF_TEMPLATE}` under `layout: {}`, and only `{REF_LAYOUT}` prints \
                 the sources it collects",
                path.display(),
                layout_of(text).unwrap_or("(none)")
            ));
        }
    })?;

    if !problems.is_empty() {
        bail!(
            "{} superscript reference(s) in the site's prose are malformed:\n{}\n\
             A reference is `{{% assign ref_src %}}` + `{{% assign ref_url %}}` + \
             `{{% include \"{REF_TEMPLATE}\" %}}`; `ref_date` and `ref_note` are optional, except \
             that a forum thread is dated by construction. The number on the page is a promise that \
             a source is printed under the text, so an empty field is a promise the page cannot \
             keep. Urls are not fetched here -- this is shape, not reachability.",
            problems.len(),
            problems.join("\n"),
        );
    }

    println!(
        "Superscript references: {} in {files} file(s), all well-formed",
        refs.len()
    );
    Ok(())
}

/// Every `[n]` marker in one blurb. Exactly what `prose.liquid` substitutes, backtick chunking
/// included, so that `a[0]` in a blurb is prose here too and this cannot report a defect the page
/// does not have. A non-canonical `[01]` yields 0 and is reported: the renderer leaves it as text.
fn blurb_markers(blurb: &str) -> Vec<usize> {
    let mut out = Vec::new();

    for (chunk, prose) in blurb.split('`').enumerate() {
        if chunk % 2 == 1 {
            continue; // inside `code`
        }
        let bytes = prose.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'[' {
                let start = i + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                if end > start && bytes.get(end) == Some(&b']') {
                    let digits = &prose[start..end];
                    let n = digits.parse::<usize>().unwrap_or(0);
                    out.push(if n.to_string() == digits { n } else { 0 });
                    i = end + 1;
                    continue;
                }
            }
            i += 1;
        }
    }

    out
}

/// The line that opens the hub or section with this id. Ids are unique across the file, which is
/// what makes a plain scan for the key enough.
fn yaml_line_of(text: &str, id: &str) -> usize {
    text.lines()
        .position(|line| {
            line.trim_start()
                .trim_start_matches("- ")
                .strip_prefix("id:")
                .is_some_and(|v| v.trim() == id)
        })
        .map_or(1, |i| i + 1)
}

/// Fail if a shelf blurb and its `sources:` list disagree, in either direction: a dangling `[n]`
/// is a number with nothing behind it, and an unreferenced source is a receipt for a claim that
/// was edited away -- which is the defect this whole mechanism exists to catch.
pub fn check_blurb_sources(path: &Path, topics: &[HubDef]) -> Result<()> {
    // Read only for line numbers: the check is on the parsed structs, so a caller that cannot see
    // the file still gets the whole check, reported at line 1.
    let text = fs::read_to_string(path).unwrap_or_default();
    let today = Utc::now().date_naive();
    let mut problems: Vec<String> = Vec::new();
    let mut count = 0usize;

    for hub in topics {
        for section in &hub.sections {
            let id = section.id;
            let at = format!(
                "  {}:{}",
                path.display(),
                yaml_line_of(&text, &id.to_string())
            );
            let markers = blurb_markers(&section.blurb);
            let listed = section.sources.len();

            for n in &markers {
                if *n == 0 || *n > listed {
                    problems.push(format!(
                        "{at} section `{id}` marks `[{n}]` in its blurb, but its `sources:` list \
                         has {listed} entr{}",
                        if listed == 1 { "y" } else { "ies" }
                    ));
                }
            }

            for (i, source) in section.sources.iter().enumerate() {
                let n = i + 1;
                count += 1;
                if !markers.contains(&n) {
                    problems.push(format!(
                        "{at} section `{id}` lists source {n} (`{}`) and no `[{n}]` in its blurb \
                         points at it",
                        source.src
                    ));
                }
                // A key with nothing after it prints a dangling em dash on the page.
                if source.note.as_deref().is_some_and(|n| n.trim().is_empty()) {
                    problems.push(format!(
                        "{at} section `{id}` source {n} has an empty `note`; leave the key out"
                    ));
                }
                let date = source.date.map(|d| d.to_string());
                check_source(
                    &format!("{at} section `{id}` source {n}"),
                    BLURB_FIELDS,
                    Some(&source.src),
                    Some(&source.url),
                    date.as_deref(),
                    today,
                    &mut problems,
                );
            }
        }
    }

    if !problems.is_empty() {
        bail!(
            "{} shelf blurb source(s) in {} do not line up:\n{}\n\
             A blurb cannot hold an include, so its claims carry bare `[1]`, `[2]` markers and the \
             receipts hang off the section as `sources:`. The two lists are positional and the \
             check runs both ways: an unreferenced source is as much a defect as a dangling marker.",
            problems.len(),
            path.display(),
            problems.join("\n"),
        );
    }

    println!("Shelf blurb sources: {count}, every marker and every source accounted for");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy::{Color, Hub, SectionDef, SourceDef, Topic};

    /// A throwaway site of `(filename, body)` pairs, with one check run over it. Uniquely named so
    /// the tests still run in parallel; filenames matter, because each check skips its own template.
    fn checked(files: &[(&str, &str)], check: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "awly-xref-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for (name, body) in files {
            fs::write(dir.join(name), body).unwrap();
        }
        let result = check(&dir);
        let _ = fs::remove_dir_all(&dir);
        result
    }

    /// The common case: one ordinary page.
    fn page(body: &str, check: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
        checked(&[("page.md", body)], check)
    }

    fn failure(body: &str, check: impl FnOnce(&Path) -> Result<()>) -> String {
        page(body, check)
            .expect_err("this page is malformed")
            .to_string()
    }

    #[test]
    fn it_finds_a_reference_in_any_assign_form() {
        let repo = "https://codeberg.org/a/b";
        for (text, id) in [
            (r#"Use {% assign xref = "pyo3" %} in prose."#, "pyo3"),
            // A repo-only entry's id is its repository URL, and either quote is legal.
            ("{% assign xref = 'https://codeberg.org/a/b' %}", repo),
            ("{% assign xref='burn' %}", "burn"),
            ("{% assign xref   =  \"burn\" %}", "burn"),
        ] {
            assert_eq!(scan(text), vec![id.to_string()], "{text}");
        }
    }

    #[test]
    fn the_card_template_is_not_a_call_site() {
        let template = "{% comment %}{% assign xref = \"pyo3\" %}{% endcomment %}\n\
                        {%- assign xref = \"\" -%}";
        let no_ids = |dir: &Path| check(dir, &HashSet::new());
        assert!(
            checked(&[(XREF_TEMPLATE, template)], no_ids).is_ok(),
            "the card template is the mechanism, not a call site"
        );
        // The skip is by filename: a real page with the same empty assign still fails.
        assert!(checked(&[("page.md", "{%- assign xref = \"\" -%}")], no_ids).is_err());
    }

    #[test]
    fn an_unknown_reference_fails_with_the_file_that_holds_it() {
        let ids: HashSet<String> = ["ndarray".to_string()].into_iter().collect();
        let err = page(r#"{% assign xref = "ndaray" %}"#, |dir| check(dir, &ids))
            .unwrap_err()
            .to_string();
        assert!(err.contains("ndaray"), "{err}");
        assert!(err.contains("page.md"), "{err}");

        page(r#"{% assign xref = "ndarray" %}"#, |dir| check(dir, &ids)).unwrap();
    }

    const GOOD: &str = r#"{% assign cite_src = "tract README" %}{% assign cite_url = "https://github.com/sonos/tract" %}{% include "cite.liquid" %}"#;

    /// A dropped field reads the way `cite.liquid` renders one: empty, not inherited.
    #[test]
    fn assignments_group_one_citation_per_include_and_never_inherit() {
        let text = format!(
            "{GOOD}\nand also {}\n{}",
            r#"{% assign cite_src = "Cloudflare" %}{% assign cite_date = "2025-08-27" %}{% assign cite_url = "https://blog.cloudflare.com/x" %}{% include "cite.liquid" %}"#,
            r#"{% assign cite_src = "r/rust" %}{% include "cite.liquid" %}"#
        );
        let cites = scan_markers(&text, CITE.fields, CITE.template);
        let field = |cite: usize, slot: usize| cites[cite].values[slot].as_deref();
        assert_eq!(cites.len(), 3);
        assert_eq!(field(0, SRC), Some("tract README"));
        assert_eq!(field(0, DATE), None, "a living document carries no date");
        assert_eq!(field(1, DATE), Some("2025-08-27"));
        assert_eq!(cites[1].line, 2);
        assert_eq!(field(2, URL), None, "dropped, not inherited");

        let err = failure(&text, check_citations);
        assert!(err.contains("has no `cite_url`"), "{err}");
        assert!(err.contains("page.md:3"), "{err}");
    }

    #[test]
    fn a_forum_thread_must_carry_a_date_and_a_living_document_need_not() {
        let forum = r#"{% assign cite_src = "r/rust" %}{% assign cite_url = "https://old.reddit.com/r/rust/comments/1eol2nd/x/" %}{% include "cite.liquid" %}"#;
        let err = failure(forum, check_citations);
        assert!(err.contains("dated by construction"), "{err}");

        // The same citation with its date, and a README with none, both pass.
        let dated = forum.replace(
            r#"{% assign cite_url"#,
            r#"{% assign cite_date = "2024-08-10" %}{% assign cite_url"#,
        );
        page(&format!("{dated}\n{GOOD}"), check_citations).unwrap();
    }

    /// Assignments nobody consumed render as nothing -- but a page opening by clearing the fields
    /// (Liquid errors on an unassigned name) leaves a legal leftover.
    #[test]
    fn assignments_with_no_include_are_an_orphan_unless_they_are_a_pages_opening_reset() {
        let err = failure(
            r#"{% assign cite_src = "Cloudflare" %}{% assign cite_url = "https://blog.cloudflare.com/x" %}"#,
            check_citations,
        );
        assert!(err.contains("with no `{% include"), "{err}");

        page(
            r#"{%- assign cite_url = "" -%}{%- assign cite_src = "" -%}{%- assign cite_date = "" -%}"#,
            check_citations,
        )
        .unwrap();
    }

    // -- the superscript marker ------------------------------------------------------------------

    const GOOD_REF: &str = r#"{% assign ref_src = "NVIDIA" %}{% assign ref_date = "2026-09-08" %}{% assign ref_url = "https://developer.nvidia.com/blog/x" %}{% assign ref_note = "the vendor" %}{% include "ref.liquid" %}"#;

    /// Wiring only -- the shape rules are `check_source`'s, tested once below. `ref_note` is
    /// scanned so a note-only call site that forgets its include is an orphan rather than silence.
    #[test]
    fn a_ref_names_its_own_fields_and_a_note_alone_is_still_an_orphan() {
        let text = format!(
            "claim{GOOD_REF}.\nand{}",
            r#"{% assign ref_src = "r/rust" %}{% include "ref.liquid" %}"#
        );
        let err = failure(&text, check_refs);
        assert!(err.contains("has no `ref_url`"), "{err}");
        assert!(err.contains("page.md:2"), "{err}");

        let lost = failure(r#"{% assign ref_note = "the vendor" %}"#, check_refs);
        assert!(lost.contains("with no `{% include"), "{lost}");
    }

    /// A number renders under any layout; the source prints under one. Both failures are invisible
    /// in a browser, which is why they are build errors.
    #[test]
    fn a_reference_needs_the_layout_that_collects_it_and_the_fence_it_splits_on() {
        let body = format!("---\nlayout: page.liquid\ntitle: About\n---\nclaim{GOOD_REF}.");
        let err = failure(&body, check_refs);
        assert!(err.contains("only `crates.liquid` prints"), "{err}");
        page(&body.replace("page.liquid", "crates.liquid"), check_refs).unwrap();

        let by_hand = failure("Some prose that writes <!--ref--> by hand.", check_refs);
        assert!(by_hand.contains("splits the page on"), "{by_hand}");
    }

    #[test]
    fn the_ref_template_and_the_layout_that_drives_it_are_not_call_sites() {
        // Both files carry the fence and an include with no `assign` in front of it.
        let mechanism = [
            (
                "ref.liquid",
                "{%- unless ref_src -%}{%- assign ref_src = \"\" -%}{%- endunless -%}<!--ref-->",
            ),
            (
                "crates.liquid",
                "{% assign ref_chunks = page.content | split: \"<!--ref-->\" %}\
                 {% include \"ref.liquid\" %}",
            ),
        ];
        checked(&mechanism, check_refs).expect("the mechanism is not a call site");

        // The skip is by filename: the same text in a page still fails.
        let with_page = [mechanism[0], mechanism[1], ("page.md", "<!--ref-->")];
        assert!(checked(&with_page, check_refs).is_err());
    }

    /// Why these are scanners and not substring searches.
    #[test]
    fn the_templates_own_locals_and_prose_naming_them_are_not_call_sites() {
        let card = r#"
            {%- assign xref_found = false -%}
            {%- for xref_c in site.data.crates_generated -%}
            {%- if xref_c.id == xref -%}{%- assign crate = xref_c -%}{%- endif -%}
            {%- endfor -%}
            {{ xref | escape }}
        "#;
        assert!(scan(card).is_empty(), "{:?}", scan(card));

        let cite = r#"
            {%- assign cite_url_len = cite_url | size -%}
            {%- assign cite_src_len = cite_src | size -%}
            See cite.liquid, or the "cite.liquid" include, for how this works.
        "#;
        let cites = scan_markers(cite, CITE.fields, CITE.template);
        assert!(cites.is_empty(), "{cites:?}");
    }

    // -- the shelf blurbs ----------------------------------------------------------------------

    fn source(src: &str, url: &str, date: Option<&str>, note: Option<&str>) -> SourceDef {
        SourceDef {
            src: src.into(),
            url: url.into(),
            date: date.map(|d| d.parse().unwrap()),
            note: note.map(str::to_string),
        }
    }

    fn nvidia() -> SourceDef {
        source(
            "NVIDIA",
            "https://developer.nvidia.com/blog/x",
            Some("2026-09-08"),
            Some("the vendor, limiting its own product"),
        )
    }

    /// One shelf checked through a throwaway `topics.yaml`, so the line number points somewhere.
    fn check_blurb(blurb: &str, sources: Vec<SourceDef>) -> Result<()> {
        let hub = HubDef {
            id: Hub::Gpu,
            short: "GPU".into(),
            title: "GPU & Accelerators".into(),
            tagline: "Driving the hardware.".into(),
            color: Color::Yellow,
            sections: vec![SectionDef {
                id: Topic::GpuKernels,
                title: "Kernel languages & GPU compilers".into(),
                color: Color::Yellow,
                blurb: blurb.into(),
                sources,
                verified: "2026-09-03".parse().unwrap(),
            }],
        };
        let stub = "- id: gpu\n  sections:\n    - id: gpu-kernels\n      blurb: x\n";
        checked(&[("topics.yaml", stub)], |dir| {
            check_blurb_sources(&dir.join("topics.yaml"), &[hub])
        })
    }

    fn blurb_failure(blurb: &str, sources: Vec<SourceDef>) -> String {
        check_blurb(blurb, sources)
            .expect_err("this shelf does not line up")
            .to_string()
    }

    /// A source no marker points at is a receipt for a claim that was edited away.
    #[test]
    fn a_blurb_and_its_sources_have_to_line_up_both_ways() {
        check_blurb("Neither half is production-ready.[1]", vec![nvidia()]).unwrap();

        let dangling = blurb_failure("Ready.[1] Also ready.[2]", vec![nvidia()]);
        assert!(dangling.contains("marks `[2]`"), "{dangling}");
        assert!(dangling.contains("topics.yaml:3"), "{dangling}");

        let unreferenced = blurb_failure("Ready.[1]", vec![nvidia(), nvidia()]);
        assert!(unreferenced.contains("lists source 2"), "{unreferenced}");
        assert!(unreferenced.contains("no `[2]`"), "{unreferenced}");
    }

    #[test]
    fn ordinary_brackets_in_a_blurb_are_not_markers() {
        let err = blurb_failure(
            "See [the book], the `a[0]` idiom, a stray [ and [01].[1]",
            vec![nvidia()],
        );
        // Only `[01]` is reported: prose.liquid would leave that one on the page as literal text.
        assert_eq!(err.matches("marks `[").count(), 1, "{err}");
        assert!(err.contains("marks `[0]`"), "{err}");
    }

    /// The one table over `check_source`, which all three vocabularies route into.
    #[test]
    fn a_source_is_held_to_the_same_shape_wherever_it_is_written() {
        let ok = "https://example.com/";
        let forum = "https://old.reddit.com/r/rust/x/";
        let next_year = (Utc::now().date_naive() + chrono::Duration::days(400)).to_string();
        for (src, url, date, note, needle) in [
            ("", ok, None, None, "empty `src`"),
            ("NVIDIA", "developer.nvidia.com", None, None, "not a url"),
            ("NVIDIA", "ftp://example.com/x", None, None, "not http(s)"),
            ("NVIDIA", ok, Some(&*next_year), None, "in the future"),
            ("r/rust", forum, None, None, "dated by construction"),
            ("NVIDIA", ok, None, Some("  "), "empty `note`"),
        ] {
            let err = blurb_failure("A claim.[1]", vec![source(src, url, date, note)]);
            assert!(err.contains(needle), "{needle}: {err}");
        }
    }
}
