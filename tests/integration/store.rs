// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use ocd::store::*;
use crate::{GitKind, GitFixture};

use anyhow::Result;
use pretty_assertions::assert_eq as pretty_assert_eq;
use sealed_test::prelude::*;
use simple_test_case::dir_cases;

fn foo() -> Result<()> {
    let git = GitFixture::new("hello", GitKind::Normal)?;
    Ok(())
}
