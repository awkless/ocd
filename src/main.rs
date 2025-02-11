// SPDX-FileCopyrightText: 2025 Jason Pena <jasonpena@awkless.com>
// SPDX-License-Identifier: MIT

fn main() {
    let mut colog = colog::default_builder();
    colog.filter(None, log::LevelFilter::Info);
    colog.init();

    log::info!("Hello from ocd!");
}
