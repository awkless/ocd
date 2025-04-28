// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

use crate::{
    model::{DeploymentKind, DirAlias},
    store::Root,
    Result
};

use sealed_test::prelude::*;
use simple_test_case::dir_cases;
use pretty_assertions::assert_eq as pretty_assert_eq;

#[sealed_test(env = [("HOME", "./"), ("XDG_DATA_HOME", "store")])]
fn smoke_root_new_init() -> Result<()> {
    let root = Root::new_init()?;
    assert!(root.path().exists());
    pretty_assert_eq!(root.deployment_kind(), &DeploymentKind::BareAlias(DirAlias::default()));

    Ok(())
}
