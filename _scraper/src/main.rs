use anyhow::{Context, Result, bail};

mod crates;
mod data;
mod github;
mod util;

use crates::CratesIo;
use data::{GeneratedCrateInfo, InputCrateInfo};
use github::Github;
use std::env;
use url::Url;
use util::{read_yaml, write_yaml};

// Some crates.io repository URLs carry a `www.` prefix, which would otherwise skip scraping.
fn is_github(repo: &Url) -> bool {
    matches!(repo.host_str(), Some("github.com" | "www.github.com"))
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args();
    let _ = args.next();
    let Some(path) = args.next() else {
        bail!("Usage: scraper <path_to_crates_yaml>");
    };

    let gh = Github::new()?;
    let crates_io = CratesIo::new()?;

    let input: Vec<InputCrateInfo> =
        read_yaml(&path).with_context(|| format!("Error reading {path}"))?;
    let mut generated = Vec::with_capacity(input.len());
    for krate in input {
        match (&krate.name, &krate.repository) {
            (Some(name), _) => println!("Processing crate {name}"),
            (None, Some(repo)) => println!("Processing repo {repo}"),
            (None, None) => {
                println!("Invalid entry: {krate:#?}");
                continue;
            }
        }

        let mut entry = GeneratedCrateInfo::from(&krate);
        if let Some(crate_name) = &krate.name {
            match crates_io.get_crate_data(crate_name).await {
                Ok(data) => entry.set_crate_data(data),
                Err(err) => eprintln!("Error getting crate data for {crate_name} - {err}"),
            }
        }
        entry.apply_overrides(&krate);

        if let Some(repo) = entry.repository(&krate)
            && is_github(&repo)
        {
            let mut parts = repo.path().trim_start_matches('/').split('/');
            match (parts.next(), parts.next()) {
                (Some(owner), Some(name)) if !owner.is_empty() && !name.is_empty() => {
                    let name = name.strip_suffix(".git").unwrap_or(name);
                    match gh.get_repo_data(owner, name).await {
                        Ok(data) => entry.repo = Some(data),
                        Err(err) => {
                            eprintln!("Error getting Github repo data for {owner}/{name} - {err}")
                        }
                    }
                }
                _ => eprintln!("Unrecognized Github repo URL: {repo}"),
            }
        }

        entry.update_score();
        generated.push(entry);
    }

    write_yaml("_data/crates_generated.yaml", generated)
}
