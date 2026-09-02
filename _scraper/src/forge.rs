//! Repository hosts: GitHub, Codeberg, and the GitLab instances in `GITLAB_HOSTS`. Reading only
//! GitHub is not an option -- whisper-rs moved to Codeberg and its GitHub mirror is flagged
//! *archived*, so a GitHub-only scraper renders a death notice for a live project.

use crate::http::Http;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

/// How many commits to read looking for a human.
const COMMIT_SCAN: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoData {
    /// `owner/name`, or `group/subgroup/name` on GitLab.
    pub name: String,
    pub url: String,
    pub host: String,
    pub description: Option<String>,
    pub stargazers_count: u32,
    /// GitHub's `pushed_at` (GitLab's `last_activity_at`, Forgejo's `updated_at`). It moves for bot
    /// pushes and fork PR branches, so it may only veto an archive, never date activity. `None` and
    /// not now when the forge returns nothing: a date that is always today can never be stale.
    pub last_commit: Option<DateTime<Utc>>,
    /// Newest commit on the default branch whose author is not a bot.
    pub last_real_commit: Option<DateTime<Utc>>,
    /// Distinct non-bot contributors, capped at the first page. A floor, never an overstatement.
    pub contributors: Option<u32>,
    /// Repository creation, standing in for "first published" on entries that are not on crates.io.
    pub created_at: Option<DateTime<Utc>>,
    /// The owner's own archived flag. `None` where the forge would not say, which is not "not
    /// archived".
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forge {
    GitHub,
    /// Forgejo/Gitea. Codeberg is the only instance the catalog uses.
    Codeberg,
    GitLab {
        host: String,
    },
}

/// The GitLab instances this scraper will talk to. An allowlist rather than a `gitlab.` prefix
/// test, which would let a merged `repository:` point the CI runner at any host registered under
/// that prefix. An instance that is not here is *unsupported*, not dead: its signals stay unknown.
pub const GITLAB_HOSTS: &[&str] = &["gitlab.com", "gitlab.freedesktop.org"];

/// A repository URL resolved to a forge and a project path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub forge: Forge,
    /// `owner/name`, or the full namespaced path on GitLab.
    pub path: String,
}

impl RepoRef {
    /// `None` for a host we do not know how to read: unknown, and neither dead nor unreachable.
    pub fn parse(url: &Url) -> Option<RepoRef> {
        let host = url.host_str()?.trim_start_matches("www.").to_lowercase();

        let segments: Vec<&str> = url
            .path()
            .trim_matches('/')
            .split('/')
            // `/-/tree/main`, `/blob/...`: a path into the repo, not part of its identity.
            .take_while(|part| !matches!(*part, "-" | "tree" | "blob" | "src"))
            .filter(|part| !part.is_empty())
            .collect();
        if segments.len() < 2 {
            return None;
        }
        let trim = |part: &str| part.trim_end_matches(".git").to_string();

        match host.as_str() {
            "github.com" => Some(RepoRef {
                forge: Forge::GitHub,
                path: format!("{}/{}", segments[0], trim(segments[1])),
            }),
            "codeberg.org" => Some(RepoRef {
                forge: Forge::Codeberg,
                path: format!("{}/{}", segments[0], trim(segments[1])),
            }),
            // Nested groups mean the project path can be more than two segments.
            _ if GITLAB_HOSTS.contains(&host.as_str()) => {
                let mut parts: Vec<String> = segments.iter().map(|part| trim(part)).collect();
                if let Some(last) = parts.last_mut() {
                    *last = last.trim_end_matches(".git").to_string();
                }
                Some(RepoRef {
                    forge: Forge::GitLab { host },
                    path: parts.join("/"),
                })
            }
            _ => None,
        }
    }

    /// The identity of a repository, for comparing two URLs that claim to be the same project.
    pub fn slug(&self) -> String {
        let host = match &self.forge {
            Forge::GitHub => "github.com",
            Forge::Codeberg => "codeberg.org",
            Forge::GitLab { host } => host,
        };
        format!("{host}/{}", self.path.to_lowercase())
    }
}

/// Names and logins that mean "a machine did this". Deliberately conservative: a false positive
/// erases a human's commit and makes a live project look dormant, while a missed bot only flatters
/// one.
const BOT_MARKERS: &[&str] = &[
    "dependabot",
    "renovate",
    "github-actions",
    "greenkeeper",
    "mergify",
    "codecov",
    "snyk-bot",
    "allcontributors",
    "pre-commit-ci",
    "semantic-release",
    "release-plz",
    "restyled",
    "imgbot",
    "whitesource",
    "scala-steward",
    "github-merge-queue",
];

fn is_bot(
    login: Option<&str>,
    kind: Option<&str>,
    name: Option<&str>,
    email: Option<&str>,
) -> bool {
    if kind == Some("Bot") {
        return true;
    }
    let candidates = [login, name, email];
    candidates.iter().flatten().any(|value| {
        let value = value.to_lowercase();
        value.contains("[bot]")
            || value.ends_with("-bot")
            || value.starts_with("bot@")
            || BOT_MARKERS.iter().any(|marker| value.contains(marker))
    })
}

/// `""` and `null` are the same fact -- nobody wrote a description -- and have to collapse here,
/// because `tier::evaluate_bar` reads the absence as evidence.
fn described(text: Option<String>) -> Option<String> {
    text.map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

// --- GitHub ---------------------------------------------------------------------------------

#[derive(Deserialize)]
struct GhRepo {
    full_name: String,
    html_url: String,
    #[serde(default)]
    description: Option<String>,
    stargazers_count: u32,
    pushed_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
    archived: Option<bool>,
}

#[derive(Deserialize)]
struct GhCommit {
    commit: GhCommitDetail,
    /// `null` when the commit email is linked to no account, which is a human until proven otherwise.
    author: Option<GhUser>,
}

#[derive(Deserialize)]
struct GhCommitDetail {
    author: Option<GhSignature>,
    committer: Option<GhSignature>,
}

#[derive(Deserialize)]
struct GhSignature {
    name: Option<String>,
    email: Option<String>,
    date: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct GhUser {
    login: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

// --- Codeberg (Forgejo) ---------------------------------------------------------------------

#[derive(Deserialize)]
struct ForgejoRepo {
    full_name: String,
    html_url: String,
    #[serde(default)]
    description: Option<String>,
    stars_count: u32,
    updated_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
    archived: Option<bool>,
}

#[derive(Deserialize)]
struct ForgejoCommit {
    commit: ForgejoCommitDetail,
    author: Option<ForgejoUser>,
}

#[derive(Deserialize)]
struct ForgejoCommitDetail {
    author: Option<GhSignature>,
    committer: Option<GhSignature>,
}

#[derive(Deserialize)]
struct ForgejoUser {
    login: Option<String>,
}

// --- GitLab ----------------------------------------------------------------------------------

#[derive(Deserialize)]
struct GlProject {
    path_with_namespace: String,
    web_url: String,
    // Unauthenticated GitLab omits or nulls several of these.
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    star_count: Option<u32>,
    #[serde(default)]
    last_activity_at: Option<DateTime<Utc>>,
    #[serde(default)]
    created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    archived: Option<bool>,
}

#[derive(Deserialize)]
struct GlCommit {
    author_name: Option<String>,
    author_email: Option<String>,
    committed_date: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
}

/// The newest human commit date in a list already ordered newest-first, and the number of distinct
/// non-bot authors seen. `None` when every commit read was a bot's, because that date would feed
/// the dormancy trigger.
fn summarize<'a>(
    commits: impl IntoIterator<Item = (bool, Option<DateTime<Utc>>, Option<&'a str>)>,
) -> (Option<DateTime<Utc>>, u32) {
    let mut newest: Option<DateTime<Utc>> = None;
    let mut authors: Vec<String> = Vec::new();
    for (bot, date, identity) in commits {
        if bot {
            continue;
        }
        if let Some(date) = date
            && newest.is_none_or(|seen| date > seen)
        {
            newest = Some(date);
        }
        if let Some(identity) = identity {
            let identity = identity.to_lowercase();
            if !authors.contains(&identity) {
                authors.push(identity);
            }
        }
    }
    (newest, authors.len() as u32)
}

/// A sub-fetch that may fail without failing the repository lookup, with the error printed instead
/// of thrown away.
fn reported<T>(result: Result<Option<T>>, what: &str, slug: &str) -> Option<T> {
    match result {
        Ok(value) => value,
        Err(err) => {
            eprintln!("  ! {what} for {slug} unreadable: {err:#}");
            None
        }
    }
}

pub struct Forges {
    http: Http,
}

/// Kept apart all the way to the signal set, because only `Gone` may archive anything.
pub enum RepoLookup {
    Found(Box<RepoData>),
    /// The forge answered 404, which is evidence `tier` is allowed to act on.
    Gone,
    /// The host is one we cannot read, or the request failed. Never a verdict.
    Unknown,
}

impl Forges {
    pub fn new(http: Http) -> Forges {
        Forges { http }
    }

    pub async fn get(&self, repo: &RepoRef) -> RepoLookup {
        let result = match &repo.forge {
            Forge::GitHub => self.github(&repo.path).await,
            Forge::Codeberg => self.codeberg(&repo.path).await,
            Forge::GitLab { host } => self.gitlab(host, &repo.path).await,
        };
        match result {
            Ok(Some(data)) => RepoLookup::Found(Box::new(data)),
            Ok(None) => RepoLookup::Gone,
            Err(err) => {
                eprintln!("  ! repo {} unreadable: {err:#}", repo.slug());
                RepoLookup::Unknown
            }
        }
    }

    async fn github(&self, path: &str) -> Result<Option<RepoData>> {
        let base = format!("https://api.github.com/repos/{path}");
        let Some(repo) = self.http.get_json::<GhRepo>("github", &base).await? else {
            return Ok(None);
        };

        let commits: Vec<GhCommit> = reported(
            self.http
                .get_json("github", &format!("{base}/commits?per_page={COMMIT_SCAN}"))
                .await,
            "commit list",
            path,
        )
        .unwrap_or_default();
        let (last_real_commit, _) = summarize(commits.iter().map(|entry| {
            let author = entry.commit.author.as_ref();
            let bot = is_bot(
                entry.author.as_ref().and_then(|user| user.login.as_deref()),
                entry.author.as_ref().and_then(|user| user.kind.as_deref()),
                author.and_then(|sig| sig.name.as_deref()),
                author.and_then(|sig| sig.email.as_deref()),
            );
            // Committer date: a rebase carries an old author date, and the question is when a
            // human's work last landed.
            let date = entry
                .commit
                .committer
                .as_ref()
                .and_then(|sig| sig.date)
                .or_else(|| author.and_then(|sig| sig.date));
            (bot, date, None)
        }));

        // All-time, so it beats counting authors in the commits we happened to read.
        let contributors: Option<Vec<GhUser>> = reported(
            self.http
                .get_json(
                    "github",
                    &format!("{base}/contributors?per_page=100&anon=0"),
                )
                .await,
            "contributor list",
            path,
        );
        let contributors = contributors.map(|list| {
            list.iter()
                .filter(|user| !is_bot(user.login.as_deref(), user.kind.as_deref(), None, None))
                .count() as u32
        });

        Ok(Some(RepoData {
            name: repo.full_name,
            url: repo.html_url,
            host: "github.com".into(),
            description: described(repo.description),
            stargazers_count: repo.stargazers_count,
            last_commit: repo.pushed_at.or(repo.created_at),
            last_real_commit,
            contributors,
            created_at: repo.created_at,
            archived: repo.archived,
        }))
    }

    async fn codeberg(&self, path: &str) -> Result<Option<RepoData>> {
        let base = format!("https://codeberg.org/api/v1/repos/{path}");
        let Some(repo) = self.http.get_json::<ForgejoRepo>("codeberg", &base).await? else {
            return Ok(None);
        };

        let commits: Vec<ForgejoCommit> = reported(
            self.http
                .get_json(
                    "codeberg",
                    &format!("{base}/commits?limit={COMMIT_SCAN}&stat=false&verification=false"),
                )
                .await,
            "commit list",
            path,
        )
        .unwrap_or_default();
        // Forgejo has no all-time contributor endpoint, so this is a floor from the commits read.
        let (last_real_commit, contributors) = summarize(commits.iter().map(|entry| {
            let author = entry.commit.author.as_ref();
            let bot = is_bot(
                entry.author.as_ref().and_then(|user| user.login.as_deref()),
                None,
                author.and_then(|sig| sig.name.as_deref()),
                author.and_then(|sig| sig.email.as_deref()),
            );
            let date = entry
                .commit
                .committer
                .as_ref()
                .and_then(|sig| sig.date)
                .or_else(|| author.and_then(|sig| sig.date));
            (bot, date, author.and_then(|sig| sig.email.as_deref()))
        }));

        Ok(Some(RepoData {
            name: repo.full_name,
            url: repo.html_url,
            host: "codeberg.org".into(),
            description: described(repo.description),
            stargazers_count: repo.stars_count,
            last_commit: repo.updated_at.or(repo.created_at),
            last_real_commit,
            contributors: (!commits.is_empty()).then_some(contributors),
            created_at: repo.created_at,
            archived: repo.archived,
        }))
    }

    async fn gitlab(&self, host: &str, path: &str) -> Result<Option<RepoData>> {
        let encoded = path.replace('/', "%2F");
        let base = format!("https://{host}/api/v4/projects/{encoded}");
        let Some(project) = self.http.get_json::<GlProject>("gitlab", &base).await? else {
            return Ok(None);
        };

        let commits: Vec<GlCommit> = reported(
            self.http
                .get_json(
                    "gitlab",
                    &format!("{base}/repository/commits?per_page={COMMIT_SCAN}"),
                )
                .await,
            "commit list",
            path,
        )
        .unwrap_or_default();
        // GitLab's commit list carries no account link, so bot detection is by name and email only.
        let (last_real_commit, contributors) = summarize(commits.iter().map(|entry| {
            let bot = is_bot(
                None,
                None,
                entry.author_name.as_deref(),
                entry.author_email.as_deref(),
            );
            let date = entry.committed_date.or(entry.created_at);
            (bot, date, entry.author_email.as_deref())
        }));

        Ok(Some(RepoData {
            name: project.path_with_namespace,
            url: project.web_url,
            host: host.to_string(),
            description: described(project.description),
            stargazers_count: project.star_count.unwrap_or(0),
            last_commit: project.last_activity_at.or(project.created_at),
            last_real_commit,
            contributors: (!commits.is_empty()).then_some(contributors),
            created_at: project.created_at,
            archived: project.archived,
        }))
    }

    pub fn http(&self) -> &Http {
        &self.http
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One `repository:` field, minus its scheme, reduced to the slug the scraper stores.
    fn slug(path: &str) -> Option<String> {
        let url = Url::parse(&format!("https://{path}")).unwrap();
        RepoRef::parse(&url).map(|repo| repo.slug())
    }

    #[test]
    fn a_repository_url_normalizes_to_a_slug_or_to_nothing() {
        // A slug already in canonical form, on each host the scraper reads.
        for path in [
            "github.com/huggingface/candle",
            "codeberg.org/sarah/faer-rs",
            "gitlab.com/termoshtt/accel",
            "gitlab.freedesktop.org/gst/gst-plugins-rs",
        ] {
            assert_eq!(slug(path).as_deref(), Some(path), "{path}");
        }

        // `www.`, a `.git` suffix and a deep link into a workspace or a group are all forms a
        // `repository:` field routinely carries -- the first is what crates.io holds for some.
        let linfa = "github.com/rust-ml/linfa";
        let group = "gitlab.com/group/sub/project";
        for (path, expected) in [
            ("www.github.com/calebwin/emu", "github.com/calebwin/emu"),
            (&format!("{linfa}.git"), linfa),
            (&format!("{linfa}/tree/main/algos"), linfa),
            (&format!("{group}/-/tree/main"), group),
        ] {
            assert_eq!(slug(path).as_deref(), Some(expected), "{path}");
        }

        // An unreadable host is unknown, never dead -- and a `gitlab.` prefix is not a GitLab we
        // have been told we may speak to.
        for path in [
            "git.sr.ht/~someone/thing",
            "example.com/",
            "gitlab.evil.example/a/b",
            "gitlab.internal.corp/a/b",
        ] {
            assert_eq!(slug(path), None, "{path}");
        }
    }

    #[test]
    fn a_failed_sub_fetch_is_unknown_rather_than_an_empty_answer() {
        let rate_limited: Result<Option<Vec<GhUser>>> = Err(anyhow::anyhow!("403 rate limited"));
        assert!(reported(rate_limited, "contributor list", "example/thing").is_none());
        assert_eq!(
            reported(Ok(Some(vec![1u32])), "commit list", "example/thing"),
            Some(vec![1])
        );
    }

    #[test]
    fn bots_are_recognized_and_humans_are_not() {
        assert!(is_bot(Some("dependabot[bot]"), Some("Bot"), None, None));
        assert!(is_bot(Some("renovate[bot]"), None, None, None));
        assert!(is_bot(None, None, Some("github-actions"), None));
        assert!(is_bot(
            None,
            None,
            None,
            Some("49699333+dependabot[bot]@users.noreply.github.com")
        ));
        assert!(!is_bot(
            Some("anowell"),
            Some("User"),
            Some("Anthony Nowell"),
            Some("anowell@gmail.com")
        ));
        assert!(!is_bot(Some("botanist"), Some("User"), None, None));
    }

    #[test]
    fn an_empty_forge_description_is_absent_not_blank() {
        assert_eq!(
            described(Some("  Fast HNSW  ".into())).as_deref(),
            Some("Fast HNSW")
        );
        assert_eq!(described(Some(String::new())), None);
        assert_eq!(described(Some("   ".into())), None);
        assert_eq!(described(None), None);
    }

    #[test]
    fn a_history_of_only_bots_reads_as_unknown() {
        let commits = vec![
            (true, Some(Utc::now()), None),
            (true, Some(Utc::now()), None),
        ];
        let (newest, contributors) = summarize(commits);
        assert_eq!(newest, None, "a bot's push is not a last real commit");
        assert_eq!(contributors, 0);
    }
}
