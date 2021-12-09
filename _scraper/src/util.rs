use anyhow::Result;
use serde::{de::DeserializeOwned, Serialize};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

const CACHE_DIR: &str = "_tmp";

pub fn read_yaml<D: DeserializeOwned, P: AsRef<Path>>(path: P) -> Result<D> {
    let yaml_file = File::open(path.as_ref())?;
    let reader = BufReader::new(yaml_file);
    Ok(serde_yaml::from_reader(reader)?)
}

pub fn write_yaml<S: Serialize, P: AsRef<Path>>(path: P, data: S) -> Result<()> {
    let mut yaml_file = File::create(path)?;
    serde_yaml::to_writer(&mut yaml_file, &data)?;
    Ok(())
}

pub fn cache_path(group: &str, key: &str) -> Result<PathBuf> {
    let dir = PathBuf::from(CACHE_DIR).join(group);
    fs::create_dir_all(&dir)?;
    Ok(dir.join(key))
}

pub fn read_cache<D: DeserializeOwned>(cache_path: &Path) -> Result<D> {
    let cache_file = File::open(cache_path)?;
    let reader = BufReader::new(cache_file);
    Ok(serde_json::from_reader(reader)?)
}

pub fn write_cache<S: Serialize>(cache_path: &Path, data: S) -> Result<()> {
    let mut cache_file = File::create(cache_path)?;
    serde_json::to_writer(&mut cache_file, &data)?;
    Ok(())
}
