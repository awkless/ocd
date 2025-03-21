// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT or Apache-2.0

use anyhow::{anyhow, Context, Result};
use toml_edit::DocumentMut;

#[derive(Default, Debug)]
pub struct Cluster {
    document: DocumentMut,
}

impl Cluster {
    // TODO: Implement this.
}
