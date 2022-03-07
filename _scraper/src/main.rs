use anyhow::{bail, Context, Result};

mod crates;
mod data;
mod github;
mod util;

use crates::CratesIo;
use data::{override_crate_data, GeneratedCrateInfo, InputCrateInfo};
use github::Github;
use std::env;
use url::Url;
use util::{read_yaml, write_yaml};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args();
    let _ = args.next();
    let path = match args.next() {
        Some(arg) => arg,
        None => bail!("Usage: scraper <path_to_crates_yaml>"),
    };

    let gh = Github::new()?;
    let crates_io = CratesIo::new()?;

    let input: Vec<InputCrateInfo> =
        read_yaml(&path).with_context(|| format!("Error reading {}", path))?;
    let mut generated = Vec::new();
    for krate in input {
        if let Some(name) = &krate.name {
            println!("Processing crate {}", name);
        } else if let Some(repo) = &krate.repository {
            println!("Processing repo {}", repo);
        } else {
            println!("Invalid entry: {:#?}", krate);
            continue;
        }

        let mut gen = GeneratedCrateInfo::from(&krate);
        if let Some(crate_name) = &krate.name {
            match crates_io.get_crate_data(crate_name).await {
                Ok(mut data) => {
                    override_crate_data(&mut data, &krate);
                    gen.krate = Some(data);
                }
                Err(err) => {
                    eprintln!("Error getting crate data for {} - {}", crate_name, err);
                }
            }
        }

        // Yes, we serialized in `override_crate_data` and then re-parse it as a Url,
        // but the upstream Crate type uses a String, and I still want to deserizlied data.yaml as
        // a Url as input validation
        let repo_opt: Option<Url> = gen
            .krate
            .as_ref()
            .and_then(|k| k.repository.as_ref().map(|r| Url::parse(r).unwrap()))
            .clone();
        if let Some(repo) = repo_opt {
            if repo.host_str() == Some("github.com") {
                // split path including both '/' and '.', because `.git` is occasionally appended to git URLs
                let parts = repo.path().split(&['/', '.'][..]).collect::<Vec<_>>();
                match gh.get_repo_data(parts[1], parts[2]).await {
                    Ok(data) => gen.repo = Some(data),
                    Err(err) => {
                        eprintln!(
                            "Error getting Github repo data for {}/{} - {}",
                            parts[1], parts[2], err
                        )
                    }
                }
            }
        }
        gen.update_score();
        generated.push(gen);
    }

    write_yaml("_data/crates_generated.yaml", generated)
}
