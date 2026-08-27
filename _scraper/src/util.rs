use anyhow::Result;
use serde::{Serialize, de::DeserializeOwned};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

const CACHE_DIR: &str = "_tmp";

pub fn read_yaml<D: DeserializeOwned, P: AsRef<Path>>(path: P) -> Result<D> {
    let reader = BufReader::new(File::open(path.as_ref())?);
    Ok(serde_yaml_ng::from_reader(reader)?)
}

pub fn write_yaml<S: Serialize, P: AsRef<Path>>(path: P, data: S) -> Result<()> {
    let writer = BufWriter::new(File::create(path)?);
    serde_yaml_ng::to_writer(writer, &data)?;
    Ok(())
}

pub fn cache_path(group: &str, key: &str) -> Result<PathBuf> {
    let dir = PathBuf::from(CACHE_DIR).join(group);
    fs::create_dir_all(&dir)?;
    Ok(dir.join(key))
}

pub fn read_cache<D: DeserializeOwned>(cache_path: &Path) -> Result<D> {
    let reader = BufReader::new(File::open(cache_path)?);
    Ok(serde_json::from_reader(reader)?)
}

pub fn write_cache<S: Serialize>(cache_path: &Path, data: S) -> Result<()> {
    let writer = BufWriter::new(File::create(cache_path)?);
    serde_json::to_writer(writer, &data)?;
    Ok(())
}
