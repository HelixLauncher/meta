/*
 * Copyright 2022-2023 kb1000
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
#![deny(rust_2018_idioms)]

use anyhow::Result;

mod forge;
mod mojang;

#[tokio::main]
async fn main() -> Result<()> {
	let client = reqwest::Client::new();

	mojang::fetch(&client).await?;

	mojang::process()?;

	Ok(())
}
