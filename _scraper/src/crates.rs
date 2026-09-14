use crate::util::{cache_path, read_cache, write_cache};
use anyhow::Result;
use crates_io_api::{AsyncClient, Crate, CrateResponse, Error};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// A budget, not a threshold: crates.io asks for one request a second and this costs one per
/// dependent, so an uncapped `ndarray` would take half an hour alone. Dependents arrive
/// most-downloaded first, so the resulting count is a floor -- the safe direction for both readers.
const OWNER_BUDGET: usize = 30;

/// Cache group for crate responses, **versioned**: every field pulled off a response is an
/// `Option`, so adding one silently re-reads every old cache entry as "unknown" instead of
/// refetching. Bump the suffix whenever the shape of what this file extracts changes.
const CRATE_CACHE: &str = "crates-v2";
/// Cached whole; without it a warm scrape still costs two crates.io requests per entry.
const REVDEP_CACHE: &str = "revdeps";

/// crates.io only reports a license per published version, not on `Crate`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CrateData {
    pub krate: Crate,
    pub license: Option<String>,
    /// The repository crates.io itself holds, kept so a pinned URL can be checked against what the
    /// registry says -- see `collision.rs`.
    pub registry_repository: Option<String>,
    /// `None` when the crate has no versions at all.
    pub latest_yanked: Option<bool>,
}

impl From<CrateResponse> for CrateData {
    fn from(response: CrateResponse) -> Self {
        // `max_stable_version` is null when every published version is a pre-release.
        let latest = response
            .crate_data
            .max_stable_version
            .as_ref()
            .and_then(|num| response.versions.iter().find(|v| &v.num == num))
            .or_else(|| response.versions.iter().find(|v| !v.yanked));

        let newest = response
            .versions
            .iter()
            .max_by_key(|version| version.created_at);

        CrateData {
            license: latest.and_then(|v| v.license.clone()),
            registry_repository: response.crate_data.repository.clone(),
            latest_yanked: newest.map(|version| version.yanked),
            krate: response.crate_data,
        }
    }
}

pub enum CrateLookup {
    Found(Box<CrateData>),
    /// A 404: never published under this name, or removed.
    Gone,
    /// The request failed. Never a verdict, and never a zero.
    Unknown,
}

/// The gap between requests that the crates.io crawler policy asks for -- and not a timeout:
/// `AsyncClient::new` takes only this and builds a client with no timeout at all, so a half-open
/// connection hangs the scrape until CI kills it with no output written.
const RATE_LIMIT: Duration = Duration::from_secs(1);
/// The same timeout `http::Http` gives every other source.
const TIMEOUT: Duration = Duration::from_secs(30);

pub struct CratesIo {
    client: AsyncClient,
    failures: AtomicUsize,
}

impl CratesIo {
    pub fn new() -> Result<CratesIo> {
        let client = reqwest::Client::builder()
            .user_agent("arewelearningyet.com build bot (anowell@gmail.com)")
            .timeout(TIMEOUT)
            .build()?;
        Ok(CratesIo {
            client: AsyncClient::with_http_client(client, RATE_LIMIT),
            failures: AtomicUsize::new(0),
        })
    }

    /// A run that could not read the registry still writes a file, and this tally is what tells CI
    /// the catalog is mostly unknowns.
    pub fn failures(&self) -> usize {
        self.failures.load(Ordering::Relaxed)
    }

    pub async fn get_crate_data(&self, crate_name: &str) -> CrateLookup {
        let path = match cache_path(CRATE_CACHE, crate_name) {
            Ok(path) => path,
            Err(err) => {
                eprintln!("  ! crate cache for {crate_name} unusable: {err:#}");
                return CrateLookup::Unknown;
            }
        };

        if let Ok(data) = read_cache::<CrateData>(&path) {
            return CrateLookup::Found(Box::new(data));
        }
        match self.client.get_crate(crate_name).await {
            Ok(response) => {
                let data = CrateData::from(response);
                let _ = write_cache(&path, &data);
                CrateLookup::Found(Box::new(data))
            }
            Err(Error::NotFound(_)) => CrateLookup::Gone,
            Err(err) => {
                eprintln!("  ! crate {crate_name} unreadable: {err}");
                self.failures.fetch_add(1, Ordering::Relaxed);
                CrateLookup::Unknown
            }
        }
    }

    /// Counted by owner so that a maintainer's ring of twenty crates depending on their own library
    /// collapses to one person. Returns `(distinct_owners, total_reverse_deps)`.
    pub async fn reverse_dependency_owners(&self, crate_name: &str) -> Result<(u32, u32)> {
        let path = cache_path(REVDEP_CACHE, crate_name)?;
        if let Ok(cached) = read_cache::<(u32, u32)>(&path) {
            return Ok(cached);
        }

        let page = self
            .client
            .crate_reverse_dependencies_page(crate_name, 1)
            .await
            .inspect_err(|_| {
                self.failures.fetch_add(1, Ordering::Relaxed);
            })?;
        let total = page.meta.total as u32;

        // One dependent crate can appear once per published version.
        let mut dependents: Vec<String> = Vec::new();
        for dependency in &page.dependencies {
            let name = &dependency.crate_version.crate_name;
            if !dependents.iter().any(|seen| seen == name) {
                dependents.push(name.clone());
            }
        }

        let mut owners: HashSet<String> = HashSet::new();
        for dependent in dependents.iter().take(OWNER_BUDGET) {
            let resolved = self.crate_owners(dependent).await.inspect_err(|_| {
                self.failures.fetch_add(1, Ordering::Relaxed);
            })?;
            for owner in resolved {
                owners.insert(owner);
            }
        }
        let counted = (owners.len() as u32, total);
        let _ = write_cache(&path, counted);
        Ok(counted)
    }

    async fn crate_owners(&self, crate_name: &str) -> Result<Vec<String>> {
        let path = cache_path("owners", crate_name)?;
        if let Ok(owners) = read_cache::<Vec<String>>(&path) {
            return Ok(owners);
        }
        let owners: Vec<String> = match self.client.crate_owners(crate_name).await {
            Ok(users) => users.into_iter().map(|user| user.login).collect(),
            // A dependent since removed is not an owner, and not a reason to give up on the crate
            // we are actually measuring.
            Err(Error::NotFound(_)) => Vec::new(),
            Err(err) => return Err(err.into()),
        };
        let _ = write_cache(&path, &owners);
        Ok(owners)
    }
}
