//! The RustSec advisory database, read for one thing: `informational = "unmaintained"`. The only
//! archive trigger that is somebody else's published judgment rather than a mechanical fact, and it
//! is trusted because an advisory is opened, discussed and merged in public.

use crate::http::Http;
use anyhow::Result;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

const TREE_URL: &str =
    "https://api.github.com/repos/rustsec/advisory-db/git/trees/main?recursive=1";

#[derive(Deserialize)]
struct Tree {
    tree: Vec<TreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
}

/// Which crates carry an `unmaintained` advisory, and which ones we could not finish reading.
pub struct Advisories {
    unmaintained: HashSet<String>,
    /// Crates with at least one advisory file that would not load: unknown, rather than clean.
    unreadable: HashSet<String>,
}

impl Advisories {
    /// `interesting` keeps this to a handful of file fetches out of the database's ~800 advisories.
    pub async fn fetch(http: &Http, interesting: &[String]) -> Result<Advisories> {
        let tree: Tree = http
            .get_json("rustsec", TREE_URL)
            .await?
            .ok_or_else(|| anyhow::anyhow!("rustsec advisory-db tree not found"))?;
        if tree.truncated {
            // Better unusable than "no advisory" for the half of the database that did not arrive.
            anyhow::bail!("rustsec advisory-db tree came back truncated");
        }

        let mut by_crate: HashMap<String, Vec<String>> = HashMap::new();
        for entry in tree.tree {
            // `crates/<name>/RUSTSEC-YYYY-NNNN.md`
            let Some(rest) = entry.path.strip_prefix("crates/") else {
                continue;
            };
            let Some((name, file)) = rest.split_once('/') else {
                continue;
            };
            if file.ends_with(".md") {
                by_crate
                    .entry(name.to_lowercase())
                    .or_default()
                    .push(entry.path.clone());
            }
        }

        let mut unmaintained = HashSet::new();
        let mut unreadable = HashSet::new();
        for name in interesting {
            let key = name.to_lowercase();
            let Some(paths) = by_crate.get(&key) else {
                continue;
            };
            for path in paths {
                let url = format!(
                    "https://raw.githubusercontent.com/rustsec/advisory-db/main/{}",
                    encode_path(path)
                );
                // Per-file, not per-run: `?` here would leave every entry in the catalog with no
                // RustSec status for the whole scrape.
                match http.get_text("rustsec", &url).await {
                    Ok(Some(body)) => {
                        if is_unmaintained(&body) {
                            unmaintained.insert(key.clone());
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        eprintln!("  ! rustsec advisory {path} unreadable: {err:#}");
                        unreadable.insert(key.clone());
                    }
                }
            }
        }

        Ok(Advisories {
            unmaintained,
            unreadable,
        })
    }

    /// `Some(false)` only when every advisory belonging to this crate was read and none said so:
    /// reporting "clean" off a file nobody saw is as much a claim as reporting the advisory.
    pub fn unmaintained(&self, crate_name: &str) -> Option<bool> {
        let key = crate_name.to_lowercase();
        if self.unmaintained.contains(&key) {
            return Some(true);
        }
        (!self.unreadable.contains(&key)).then_some(false)
    }
}

/// A third party's string going into a URL: a `?` or `#` in one would end the path early and turn
/// the rest into a query, which is a request for a different file that can still answer 200.
fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The other informational kinds -- `unsound`, `notice` -- are not archive triggers.
fn is_unmaintained(body: &str) -> bool {
    body.lines()
        .map(str::trim)
        .take_while(|line| !line.starts_with("# "))
        .any(|line| {
            line.starts_with("informational")
                && line.split('=').nth(1).is_some_and(|value| {
                    value
                        .trim()
                        .trim_matches('"')
                        .eq_ignore_ascii_case("unmaintained")
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNMAINTAINED: &str = r#"```toml
[advisory]
id = "RUSTSEC-2021-0139"
package = "ansi_term"
date = "2021-08-18"
informational = "unmaintained"
```

# ansi_term is Unmaintained
"#;

    const UNSOUND: &str = r#"```toml
[advisory]
id = "RUSTSEC-2021-0145"
package = "atty"
informational = "unsound"
```

# Potential unaligned read
"#;

    fn index(unmaintained: &[&str], unreadable: &[&str]) -> Advisories {
        Advisories {
            unmaintained: unmaintained.iter().map(|s| s.to_string()).collect(),
            unreadable: unreadable.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn one_unreadable_advisory_costs_one_crate_and_not_the_index() {
        let advisories = index(&["ansi_term"], &["failure"]);
        assert_eq!(advisories.unmaintained("ansi_term"), Some(true));
        assert_eq!(advisories.unmaintained("ANSI_term"), Some(true));
        assert_eq!(advisories.unmaintained("serde"), Some(false));
        assert_eq!(advisories.unmaintained("failure"), None);
    }

    #[test]
    fn an_advisory_path_is_escaped_before_it_reaches_the_url() {
        assert_eq!(
            encode_path("crates/ansi_term/RUSTSEC-2021-0139.md"),
            "crates/ansi_term/RUSTSEC-2021-0139.md"
        );
        assert_eq!(
            encode_path("crates/odd name?x=1/RUSTSEC.md"),
            "crates/odd%20name%3Fx%3D1/RUSTSEC.md"
        );
    }

    #[test]
    fn only_unmaintained_counts() {
        assert!(is_unmaintained(UNMAINTAINED));
        assert!(!is_unmaintained(UNSOUND));
        assert!(!is_unmaintained("```toml\n[advisory]\nid = \"X\"\n```\n"));
    }
}
