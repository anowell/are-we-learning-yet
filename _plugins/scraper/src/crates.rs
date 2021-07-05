use crates_io_api::{AsyncClient, Crate};
use anyhow::Result;
use std::time::Duration;
use crate::util::{cache_path, read_cache, write_cache};

pub struct CratesIo {
    client: AsyncClient,
}

impl CratesIo {
    pub fn new() -> Result<CratesIo> {
        let client = AsyncClient::new("arewelearningyet.com build bot (anowell@gmail.com)", Duration::from_secs(1))?;
        Ok(CratesIo { client })
    }

    async fn fetch_crate_data(&self, crate_name: &str) -> Result<Crate> {
        let data = self.client.get_crate(crate_name).await?;
        Ok(data.crate_data)
    }

    pub async fn get_crate_data(&self, crate_name: &str) -> Result<Crate> {
       let cache_path = cache_path("crates", crate_name)?;

       let data = match read_cache(&cache_path) {
           Ok(data) => data,
           Err(_) => {
               let data = self.fetch_crate_data(crate_name).await?;
               let _ = write_cache(&cache_path, &data);
               data
           }
       };

       Ok(data)
    }
}
