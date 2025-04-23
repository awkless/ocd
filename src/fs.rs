// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

//! File system interaction.
//!
//! This module provides basic utilitis for interacting with the user's file system. Nothing
//! special here.

use crate::{Error, Result};

use std::{
    fs::read_to_string,
    path::Path,
};

pub fn read_to_config<C>(path: impl AsRef<Path>) -> Result<C>
where
    C: std::str::FromStr<Err = Error>,
{ 
    let data = match read_to_string(path.as_ref()) {
        Ok(data) => Ok(data),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err),
    }?;

    data.parse::<C>()
}
