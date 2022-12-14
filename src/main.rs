/*
 * Copyright 2022 kb1000
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use anyhow::Result;

mod mojang;

#[tokio::main]
async fn main() -> Result<()> {
	let client = reqwest::Client::new();

	mojang::fetch(&client).await?;

	mojang::process()?;
	Ok(())
}
