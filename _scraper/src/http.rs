//! One cached HTTP layer for every source that is not crates.io. **404 is data; anything else is
//! not** -- a missing repository is a fact an archive trigger may act on, while a timeout or a rate
//! limit is unknown, hence `Option<T>` inside `Result<T>` rather than collapsing both into an
//! error. The cache stores raw bodies, so changing a struct re-parses rather than re-fetches.

use crate::util::{cache_path, read_cache, write_cache};
use anyhow::{Context, Result};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const USER_AGENT: &str = "arewelearningyet.com build bot (anowell@gmail.com)";
/// Floor on the gap between two live requests. Politeness: no source here demands it.
const MIN_REQUEST_GAP: Duration = Duration::from_millis(150);

/// How long a 404 may stay in `_tmp/`. It is the only cached status that feeds an archive trigger,
/// so cached forever, a repository that went private for an afternoon archives its entry and never
/// recovers without `just clean`. A day, to match the date in `build.yml`'s CI cache key.
const NOT_FOUND_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// The longest a rate-limit reset is worth waiting for. GitHub's primary limit resets on a rolling
/// hour; the secondary one, which a well-behaved scraper actually trips, resets in seconds.
const MAX_RATE_LIMIT_WAIT: Duration = Duration::from_secs(15 * 60);

/// Two, because a second wait only helps if something else spent the quota the first one bought.
const MAX_RATE_LIMIT_WAITS: usize = 2;

#[derive(Serialize, Deserialize)]
struct CachedResponse {
    found: bool,
    body: String,
    /// Unix seconds. `None` is an entry written before the TTL existed, and counts as expired.
    #[serde(default)]
    fetched_at: Option<i64>,
}

impl CachedResponse {
    fn usable(&self, now: i64) -> bool {
        if self.found {
            return true;
        }
        self.fetched_at
            .is_some_and(|at| now.saturating_sub(at) < NOT_FOUND_TTL.as_secs() as i64)
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}

pub struct Http {
    client: reqwest::Client,
    github_token: Option<String>,
    last_request: Mutex<Option<Instant>>,
    failures: AtomicUsize,
}

impl Http {
    pub fn new(github_token: Option<String>) -> Result<Http> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Http {
            client,
            github_token,
            last_request: Mutex::new(None),
            failures: AtomicUsize::new(0),
        })
    }

    /// Without this, a scrape where every call failed publishes a catalog of unknowns with nothing
    /// saying so.
    pub fn failures(&self) -> usize {
        self.failures.load(Ordering::Relaxed)
    }

    fn throttle(&self) -> Option<Duration> {
        let mut last = self.last_request.lock().expect("http throttle mutex");
        let wait = last.and_then(|at| MIN_REQUEST_GAP.checked_sub(at.elapsed()));
        *last = Some(Instant::now() + wait.unwrap_or_default());
        wait
    }

    async fn send(&self, url: &str) -> Result<reqwest::Response> {
        if let Some(wait) = self.throttle() {
            tokio::time::sleep(wait).await;
        }

        let mut request = self.client.get(url);
        // The token only ever goes to GitHub's own API.
        if let Some(token) = &self.github_token
            && url.starts_with("https://api.github.com/")
        {
            request = request.bearer_auth(token);
        }

        request
            .send()
            .await
            .with_context(|| format!("requesting {url}"))
    }

    /// Fetch a body, cache it, and return `None` for a definitive 404. `group` is the `_tmp/`
    /// subdirectory; the key is the URL, so two callers asking for the same thing share a response.
    pub async fn get_text(&self, group: &str, url: &str) -> Result<Option<String>> {
        let result = self.fetch(group, url).await;
        if result.is_err() {
            self.failures.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    async fn fetch(&self, group: &str, url: &str) -> Result<Option<String>> {
        let path = cache_path(group, &cache_key(url))?;
        if let Ok(cached) = read_cache::<CachedResponse>(&path)
            && cached.usable(now_unix())
        {
            return Ok(cached.found.then_some(cached.body));
        }

        let mut response = self.send(url).await?;
        // The one failure worth waiting out rather than reporting: past the quota ceiling every
        // remaining call fails.
        for _ in 0..MAX_RATE_LIMIT_WAITS {
            let Some(wait) = rate_limit_wait(response.status(), response.headers(), now_unix())
            else {
                break;
            };
            if wait > MAX_RATE_LIMIT_WAIT {
                anyhow::bail!(
                    "{url} is rate limited for another {}s, past the {}s this run will wait",
                    wait.as_secs(),
                    MAX_RATE_LIMIT_WAIT.as_secs()
                );
            }
            eprintln!(
                "  ~ rate limited ({}); waiting {}s for the window to reset",
                response.status(),
                wait.as_secs()
            );
            tokio::time::sleep(wait).await;
            response = self.send(url).await?;
        }
        let status = response.status();

        if status == reqwest::StatusCode::NOT_FOUND {
            let _ = write_cache(
                &path,
                CachedResponse {
                    found: false,
                    body: String::new(),
                    fetched_at: Some(now_unix()),
                },
            );
            return Ok(None);
        }
        // Unknown, and unknown must not be cached, or a transient failure freezes into the catalog.
        if !status.is_success() {
            anyhow::bail!("{url} returned {status}");
        }

        let body = response.text().await?;
        let _ = write_cache(
            &path,
            CachedResponse {
                found: true,
                body: body.clone(),
                fetched_at: Some(now_unix()),
            },
        );
        Ok(Some(body))
    }

    pub async fn get_json<T: DeserializeOwned>(&self, group: &str, url: &str) -> Result<Option<T>> {
        let Some(body) = self.get_text(group, url).await? else {
            return Ok(None);
        };
        match serde_json::from_str(&body) {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                // As much a failed fetch as a 500, and a caller swallowing it into an empty vec
                // would otherwise lose it.
                self.failures.fetch_add(1, Ordering::Relaxed);
                Err(anyhow::Error::new(err)).with_context(|| format!("decoding {url}"))
            }
        }
    }
}

/// How long to wait, for a response that is a rate limit rather than a refusal. GitHub's primary
/// quota returns 403/429 with `X-RateLimit-Remaining: 0` and a reset epoch; its abuse-detection
/// limit returns 403 with `Retry-After`. A plain 403 -- private repo, blocked agent -- must fail
/// immediately rather than sleeping through a permission error.
fn rate_limit_wait(status: reqwest::StatusCode, headers: &HeaderMap, now: i64) -> Option<Duration> {
    if status != reqwest::StatusCode::FORBIDDEN && status != reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return None;
    }
    let header = |name: &str| -> Option<i64> {
        headers.get(name)?.to_str().ok()?.trim().parse::<i64>().ok()
    };
    if header("x-ratelimit-remaining") == Some(0) {
        let reset = header("x-ratelimit-reset")?;
        // A second of slack: a reset timestamp that has just passed still refuses if we race it.
        return Some(Duration::from_secs(
            reset.saturating_sub(now).max(0) as u64 + 1,
        ));
    }
    let retry_after = header("retry-after")?;
    Some(Duration::from_secs(retry_after.max(0) as u64 + 1))
}

/// The readable prefix is for humans poking at `_tmp/`; the hash is what makes it unambiguous.
fn cache_key(url: &str) -> String {
    let readable: String = url
        .trim_start_matches("https://")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in url.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{}-{hash:016x}", &readable[..readable.len().min(96)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).expect("header name"),
                value.parse().expect("header value"),
            );
        }
        map
    }

    #[test]
    fn a_cached_404_expires_and_a_cached_body_does_not() {
        let day = NOT_FOUND_TTL.as_secs() as i64;
        let gone = |age: i64| CachedResponse {
            found: false,
            body: String::new(),
            fetched_at: Some(1_000_000 - age),
        };
        assert!(gone(60).usable(1_000_000));
        assert!(!gone(day).usable(1_000_000), "a day-old 404 is re-asked");
        assert!(
            !CachedResponse {
                found: false,
                body: String::new(),
                fetched_at: None,
            }
            .usable(1_000_000)
        );
        // A body is not a verdict about anything, so it keeps the permanent cache.
        assert!(
            CachedResponse {
                found: true,
                body: "{}".into(),
                fetched_at: Some(0),
            }
            .usable(1_000_000)
        );
    }

    #[test]
    fn only_a_rate_limit_sleeps_and_it_sleeps_past_the_reset() {
        let wait =
            |status, pairs: &[(&str, &str)]| rate_limit_wait(status, &headers(pairs), 1_000_000);
        let exhausted =
            |reset: &'static str| [("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", reset)];
        let secs = |n| Some(Duration::from_secs(n));

        // The primary limit: wait out the reset, plus a second against clock skew.
        assert_eq!(
            wait(StatusCode::FORBIDDEN, &exhausted("1000300")),
            secs(301)
        );
        // 429 carries the same headers and means the same thing.
        let same = wait(StatusCode::TOO_MANY_REQUESTS, &exhausted("1000010"));
        assert_eq!(same, secs(11));
        // A reset already in the past still waits, rather than spinning on the next 403.
        assert_eq!(wait(StatusCode::FORBIDDEN, &exhausted("999999")), secs(1));
        // The secondary limit carries a retry-after instead.
        assert_eq!(
            wait(StatusCode::FORBIDDEN, &[("retry-after", "60")]),
            secs(61)
        );

        // Not rate limits: an authorization failure, a 403 with budget left, and a 500 that
        // suggests a retry. Sleeping on any of these stalls the run for nothing.
        assert_eq!(wait(StatusCode::FORBIDDEN, &[]), None);
        assert_eq!(
            wait(StatusCode::FORBIDDEN, &[("x-ratelimit-remaining", "4999")]),
            None
        );
        assert_eq!(
            wait(StatusCode::INTERNAL_SERVER_ERROR, &[("retry-after", "5")]),
            None
        );
    }
}
