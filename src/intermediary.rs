use std::{collections::BTreeSet, fs, path::Path, str::FromStr};

use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::{stream, StreamExt, TryStreamExt};
use helixlauncher_meta::{
	component::{Component, ComponentDependency, ConditionalClasspathEntry, Download},
	index::Index,
	util::GradleSpecifier,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{get_hash, get_size};

const CONCURRENT_FETCH_LIMIT: usize = 5;
pub async fn fetch(client: &Client) -> Result<()> {
	let upstream_base = Path::new("upstream/intermediary");

	fs::create_dir_all(&upstream_base).unwrap();

	stream::iter(get_versions(client).await?)
		.map(|version| async { fetch_version(version, client, &upstream_base).await })
		.buffer_unordered(CONCURRENT_FETCH_LIMIT)
		.try_collect::<()>()
		.await?;
	Ok(())
}

async fn fetch_version(version: String, client: &Client, upstream_base: &Path) -> Result<()> {
	let version_path = upstream_base.join(format!("{}.json", version));
	if version_path.try_exists()? {
		return Ok(());
	}

	let library = crate::Library {
		name: GradleSpecifier::from_str(&format!("net.fabricmc:intermediary:{version}")).unwrap(),
		url: "https://maven.fabricmc.net/".into(),
	};
	let download = Download {
		name: library.name.clone(),
		url: library.name.to_url(&library.url),
		hash: get_hash(client, &library).await?,
		size: get_size(client, &library).await?.try_into().unwrap(),
	};

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

	let download = DownloadWithReleaseTime {
		download,
		release_time,
	};

	fs::write(version_path, serde_json::to_string_pretty(&download)?)?;

	Ok(())
}

pub fn process() -> Result<()> {
	let out_base = Path::new("out/net.fabricmc.intermediary");
	let upstream_base = Path::new("upstream/intermediary");
	fs::create_dir_all(out_base)?;

	let mut index: Index = vec![];

	for version_meta in fs::read_dir(upstream_base)? {
		let version_meta: DownloadWithReleaseTime =
			serde_json::from_str(&fs::read_to_string(version_meta?.path())?)?;

		let classpath = vec![ConditionalClasspathEntry::All(
			version_meta.download.name.clone(),
		)];

		let component = Component {
			format_version: 1,
			assets: None,
			conflicts: vec![],
			id: "net.fabricmc.intermediary".into(),
			jarmods: vec![],
			natives: vec![],
			release_time: version_meta.release_time,
			version: version_meta.download.name.version.clone(),
			traits: BTreeSet::new(),
			requires: vec![ComponentDependency {
				id: "net.minecraft".into(),
				version: Some(version_meta.download.name.version.clone()),
			}],
			game_jar: None,
			main_class: None,
			game_arguments: vec![],
			classpath,
			downloads: vec![version_meta.download],
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
	// maven: GradleSpecifier,
	version: String,
	// stable: bool,
}

#[derive(Serialize, Deserialize)]
struct DownloadWithReleaseTime {
	download: Download,
	release_time: DateTime<Utc>,
}
