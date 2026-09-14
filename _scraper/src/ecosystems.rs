//! ecosyste.ms `dependent_repos_count`: repositories with a manifest referencing the package.
//! Rendered as a fact, in neither the bar nor the ranking, because it reads lockfile depth rather
//! than use -- see `score`. Free and keyless; it asks only that callers identify themselves.

use crate::http::Http;
use anyhow::Result;
use serde::Deserialize;

#[derive(Deserialize)]
struct Package {
    dependent_repos_count: Option<u32>,
}

/// `None` means nobody knows, which includes "not indexed by ecosyste.ms yet". Never zero.
pub async fn dependent_repos(http: &Http, crate_name: &str) -> Result<Option<u32>> {
    let url =
        format!("https://packages.ecosyste.ms/api/v1/registries/crates.io/packages/{crate_name}");
    let package: Option<Package> = http.get_json("ecosystems", &url).await?;
    Ok(package.and_then(|package| package.dependent_repos_count))
}
