use std::{collections::BTreeSet, fs, path::Path, str::FromStr};

use anyhow::Result;
use chrono::DateTime;
use helixlauncher_meta::{
	component::{
		Component, ComponentDependency, ConditionalClasspathEntry, Dependencies, Download,
	},
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

		let release_time = DateTime::parse_from_rfc2822(
			// TODO: This does one more request than necessary, should get_size or get_hash be merged into this?
			client
				.head(library.name.to_url(&library.url))
				.header("User-Agent", "helixlauncher-meta")
				.send()
				.await?
				.headers()
				.get("last-modified")
				.expect("Cannot handle servers returning no last-modified")
				.to_str()?,
		)
		.expect(&format!(
			"Error parsing last-modified header of {}",
			library.name.to_url(&library.url)
		))
		.into();

		let classpath = vec![ConditionalClasspathEntry::All(library.name)];

		let component = Component {
			format_version: 1,
			assets: None,
			dependencies: Dependencies {
				requires: vec![ComponentDependency {
					id: "net.minecraft".into(),
					version: Some(version.clone()),
				}],
				conflicts: vec![],
				optional: vec![],
			},
			provides: vec![],
			id: "net.fabricmc.intermediary".into(),
			jarmods: vec![],
			natives: vec![],
			release_time,
			version,
			traits: BTreeSet::new(),
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
		.header("User-Agent", "helixlauncher-meta")
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
