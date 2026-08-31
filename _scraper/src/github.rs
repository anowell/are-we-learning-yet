use crate::util::{cache_path, read_cache, write_cache};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use octocrab::Octocrab;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoData {
    pub name: String,
    pub stargazers_count: u32,
    pub last_commit: DateTime<Utc>,
    // No serde default: an older cache entry must miss and refetch rather than read as `false`.
    pub archived: bool,
}

// octocrab's own Display for a failed request is just "GitHub"; the status and
// message that say what actually went wrong live on the source error.
fn describe_error(err: octocrab::Error) -> anyhow::Error {
    match err {
        octocrab::Error::GitHub { source, .. } => {
            anyhow!("{} ({})", source.message, source.status_code)
        }
        err => err.into(),
    }
}

pub struct Github {
    client: Octocrab,
}

impl Github {
    pub fn new() -> Result<Github> {
        let token = env::var("GITHUB_TOKEN").context("GITHUB_TOKEN has not been set")?;
        let client = Octocrab::builder().personal_token(token).build()?;
        Ok(Github { client })
    }

    // Uses REST rather than GraphQL: the Actions GITHUB_TOKEN is denied the
    // GraphQL `stargazers` connection on repos other than its own.
    async fn fetch_remote_repo_data(&self, username: &str, repo: &str) -> Result<RepoData> {
        let repository = self
            .client
            .repos(username, repo)
            .get()
            .await
            .map_err(describe_error)?;

        Ok(RepoData {
            name: format!("{username}/{repo}"),
            stargazers_count: repository
                .stargazers_count
                .ok_or_else(|| anyhow!("no stargazer count returned"))?,
            last_commit: repository
                .pushed_at
                .ok_or_else(|| anyhow!("no push timestamp returned"))?,
            archived: repository.archived.unwrap_or(false),
        })
    }

    pub async fn get_repo_data(&self, username: &str, repo: &str) -> Result<RepoData> {
        let cache_path = cache_path("github", &format!("{username}--{repo}"))?;

        match read_cache(&cache_path) {
            Ok(data) => Ok(data),
            Err(_) => {
                let data = self.fetch_remote_repo_data(username, repo).await?;
                let _ = write_cache(&cache_path, &data);
                Ok(data)
            }
        }
    }
}
