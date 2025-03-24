// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

use anyhow::{anyhow, Result};
use mkdirp::mkdirp;
use std::{io::Read, path::{Path, PathBuf}};

#[derive(Debug, Clone)]
pub struct DirLayout {
    home: PathBuf,
    config: PathBuf,
    data: PathBuf,
}

impl DirLayout {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().ok_or(anyhow!("Cannot find home directory"))?;
        let config = dirs::config_dir().ok_or(anyhow!("Cannot find config directory"))?.join("ocd");
        let data = dirs::data_dir().ok_or(anyhow!("Cannot find data directory"))?.join("ocd");

        mkdirp(&config)?;
        mkdirp(&data)?;

        Ok(Self { home, config, data })
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn config(&self) -> &Path {
        &self.config
    }

    pub fn data(&self) -> &Path {
        &self.data
    }
}

pub fn read_config<C>(path: impl AsRef<Path>, dirs: &DirLayout) -> Result<C>
where
    C: std::str::FromStr<Err = anyhow::Error>,
{
    let full_path = dirs.config().join(path.as_ref());
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(false)
        .read(true)
        .create(true)
        .open(full_path)?;

    let mut buffer = String::new();
    file.read_to_string(&mut buffer)?;

    let config: C = buffer.parse()?;
    Ok(config)
}
