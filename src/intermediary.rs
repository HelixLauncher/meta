use std::{collections::BTreeSet, fs, path::Path, str::FromStr};

use anyhow::Result;
use chrono::Utc;
use helixlauncher_meta::{
	component::{Component, ComponentDependency, ConditionalClasspathEntry, Download},
	index::Index,
	util::GradleSpecifier,
};
use reqwest::Client;
use serde::Deserialize;

use crate::{get_hash, get_size};

pub async fn process(client: &Client) -> Result<()> {
	let out_base = Path::new("out/net.fabricmc.intermediary");
	fs::create_dir_all(out_base)?;

	let mut index: Index = vec![];

	for version in get_versions(client).await? {
		let library = crate::Library {
			name: GradleSpecifier::from_str(&format!("net.fabricmc:intermediary:{version}"))
				.unwrap(),
			url: "https://maven.fabricmc.net/".into(),
		};
		let downloads = vec![Download {
			name: library.name.clone(),
			url: library.name.to_url(&library.url),
			hash: get_hash(client, &library).await?,
			size: get_size(client, &library).await?.try_into().unwrap(),
		}];
		let classpath = vec![ConditionalClasspathEntry::All(library.name)];

		let component = Component {
			format_version: 1,
			assets: None,
			conflicts: vec![],
			id: "net.fabricmc.intermediary".into(),
			jarmods: vec![],
			natives: vec![],
			release_time: Utc::now(),
			version: version.clone(),
			traits: BTreeSet::new(),
			requires: vec![ComponentDependency {
				id: "net.minecraft".into(),
				version: Some(version),
			}],
			game_jar: None,
			main_class: None,
			game_arguments: vec![],
			classpath,
			downloads,
		};

		fs::write(
			out_base.join(format!("{}.json", component.version)),
			serde_json::to_string_pretty(&component)?,
		)?;

		index.push(component.into());
	}

	index.sort_by(|x, y| y.release_time.cmp(&x.release_time));

	fs::write(
		out_base.join("index.json"),
		serde_json::to_string_pretty(&index)?,
	)?;

	Ok(())
}

async fn get_versions(client: &Client) -> Result<Vec<String>> {
	let response: Vec<IntermediaryVersionData> = client
		.get("https://meta.fabricmc.net/v2/versions/intermediary")
		.header("User-Agent", "helixlauncher-meta (prototype)")
		.send()
		.await?
		.json()
		.await?;
	Ok(response.into_iter().map(|v| v.version).collect())
}

#[derive(Deserialize)]
struct IntermediaryVersionData {
	maven: GradleSpecifier,
	version: String,
	stable: bool,
}
