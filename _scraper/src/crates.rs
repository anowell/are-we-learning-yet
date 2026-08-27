use crate::util::{cache_path, read_cache, write_cache};
use anyhow::Result;
use crates_io_api::{AsyncClient, Crate, CrateResponse};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// crates.io only reports a license per published version, not on `Crate`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CrateData {
    pub krate: Crate,
    pub license: Option<String>,
}

impl From<CrateResponse> for CrateData {
    fn from(response: CrateResponse) -> Self {
        let latest = response
            .crate_data
            .max_stable_version
            .as_ref()
            .and_then(|num| response.versions.iter().find(|v| &v.num == num))
            .or_else(|| response.versions.iter().find(|v| !v.yanked));

        CrateData {
            license: latest.and_then(|v| v.license.clone()),
            krate: response.crate_data,
        }
    }
}

pub struct CratesIo {
    client: AsyncClient,
}

impl CratesIo {
    pub fn new() -> Result<CratesIo> {
        let client = AsyncClient::new(
            "arewelearningyet.com build bot (anowell@gmail.com)",
            Duration::from_secs(1),
        )?;
        Ok(CratesIo { client })
    }

    async fn fetch_crate_data(&self, crate_name: &str) -> Result<CrateData> {
        Ok(self.client.get_crate(crate_name).await?.into())
    }

    pub async fn get_crate_data(&self, crate_name: &str) -> Result<CrateData> {
        let cache_path = cache_path("crates", crate_name)?;

        match read_cache(&cache_path) {
            Ok(data) => Ok(data),
            Err(_) => {
                let data = self.fetch_crate_data(crate_name).await?;
                let _ = write_cache(&cache_path, &data);
                Ok(data)
            }
        }
    }
}
