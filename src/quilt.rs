use std::{
	collections::BTreeSet,
	fs, iter,
	path::{Path, PathBuf},
	str::FromStr,
};

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use futures::{stream, StreamExt, TryStreamExt};
use helixlauncher_meta::{
	component::{Component, ComponentDependency, ConditionalClasspathEntry, Download},
	index::Index,
	util::GradleSpecifier,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::Library;

const CONCURRENT_FETCH_LIMIT: usize = 5;

pub async fn fetch(client: &Client) -> Result<()> {
	let upstream_base = Path::new("upstream/quilt");
	let versions_base = upstream_base.join("versions");
	let downloads_base = upstream_base.join("downloads");

	fs::create_dir_all(&versions_base).unwrap();
	fs::create_dir_all(&downloads_base).unwrap();

	stream::iter(get_loader_versions(client).await?)
		.map(|loader_version| async {
			let version_meta = fetch_version(&loader_version, client, &versions_base).await?;
			if let Some(version_meta) = version_meta {
				fetch_downloads(loader_version, version_meta, client, &downloads_base).await
			} else {
				Ok(())
			}
		})
		.buffer_unordered(CONCURRENT_FETCH_LIMIT)
		.try_collect::<()>()
		.await?;
	Ok(())
}

async fn fetch_version(
	loader_version: &String,
	client: &Client,
	versions_base: &PathBuf,
) -> Result<Option<LoaderMetaWithReleaseTime>> {
	if loader_version == "0.17.5-beta.4" {
		// This version's meta is very broken and I hate it
		return Ok(None);
	}

	let version_path = versions_base.join(format!("{}.json", loader_version));
	if version_path.try_exists()? {
		return Ok(Some(serde_json::from_str(&fs::read_to_string(
			version_path,
		)?)?));
	}

	let response = client.get(format!("https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-loader/{loader_version}/quilt-loader-{loader_version}.json"))
            .header("User-Agent", "helixlauncher-meta")
            .send().await?;

	let release_time = Utc
		.timestamp_millis_opt(
			response
				.headers()
				.get("quilt-last-modified-timestamp")
				.context("Error quilt did not provide release date in metadata")?
				.to_str()?
				.parse()?,
		)
		.single()
		.context("unable to parse release timestamp")?;

	let response: LoaderMeta = response.json().await?;
	let response = LoaderMetaWithReleaseTime {
		meta: response,
		release_time,
	};

	serde_json::to_writer_pretty(fs::File::create(version_path)?, &response)?;
	Ok(Some(response))
}

async fn fetch_downloads(
	loader_version: String,
	loader_meta: LoaderMetaWithReleaseTime,
	client: &Client,
	downloads_base: &PathBuf,
) -> Result<()> {
	let downloads_path = downloads_base.join(format!("{}.json", loader_version));
	if downloads_path.try_exists()? {
		return Ok(());
	}

	let libraries = loader_meta
		.meta
		.libraries
		.common
		.into_iter()
		.chain(iter::once(crate::Library {
			name: GradleSpecifier::from_str(&format!("org.quiltmc:quilt-loader:{loader_version}"))
				.unwrap(),
			url: "https://maven.quiltmc.org/repository/release/".into(),
		}));

	let downloads = stream::iter(libraries)
		.map(|library| library_to_download(client, library))
		.buffer_unordered(CONCURRENT_FETCH_LIMIT)
		.try_collect::<Vec<Download>>()
		.await?;

	serde_json::to_writer_pretty(fs::File::create(downloads_path)?, &downloads)?;

	Ok(())
}

async fn library_to_download(client: &Client, library: Library) -> Result<Download> {
	Ok(Download {
		name: library.name.clone(),
		url: library.name.to_url(&library.url),
		hash: crate::get_hash(client, &library).await?,
		size: crate::get_size(client, &library).await?.try_into().unwrap(),
	})
}

pub fn process() -> Result<()> {
	let upstream_base = Path::new("upstream/quilt");
	let versions_base = upstream_base.join("versions");
	let downloads_base = upstream_base.join("downloads");
	let out_base = Path::new("out/org.quiltmc.quilt-loader");
	fs::create_dir_all(out_base)?;

	let mut index: Index = vec![];

	for loader_meta in fs::read_dir(versions_base)? {
		let loader_meta = loader_meta?;
		let loader_version = loader_meta.file_name().clone().to_string_lossy()
			[..loader_meta.file_name().len() - 5]
			.to_string();
		let downloads: Vec<Download> = serde_json::from_str(&fs::read_to_string(
			&downloads_base.join(loader_meta.file_name()),
		)?)?;
		let loader_meta: LoaderMetaWithReleaseTime =
			serde_json::from_str(&fs::read_to_string(loader_meta.path())?)?;

		let classpath = downloads
			.iter()
			.map(|download| ConditionalClasspathEntry::All(download.name.clone()))
			.collect();

		let component = Component {
			format_version: 1,
			assets: None,
			conflicts: vec![],
			id: "org.quiltmc.quilt-loader".into(),
			jarmods: vec![],
			natives: vec![],
			release_time: loader_meta.release_time,
			version: loader_version,
			traits: BTreeSet::new(),
			requires: vec![
				ComponentDependency {
					id: "net.minecraft".into(),
					version: None,
				},
				ComponentDependency {
					id: "net.fabricmc.intermediary".into(),
					version: None,
				},
			],
			game_jar: None,
			main_class: Some(loader_meta.meta.main_class.client),
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

async fn get_loader_versions(client: &Client) -> Result<Vec<String>> {
	let response: Vec<LoaderVersionData> = client
		.get("https://meta.quiltmc.org/v3/versions/loader")
		.header("User-Agent", "helixlauncher-meta")
		.send()
		.await?
		.json()
		.await?;
	Ok(response.into_iter().map(|x| x.version).collect())
}

#[derive(Deserialize, Debug)]
struct LoaderVersionData {
	// separator: String,
	// build: i32,
	// maven: GradleSpecifier,
	version: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Libraries {
	client: Vec<Library>,
	common: Vec<Library>,
	server: Vec<Library>,
}

#[derive(Serialize, Deserialize, Debug)]
struct MainClass {
	client: String,
	server: String,
	#[serde(rename = "serverLauncher")]
	server_launcher: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct LoaderMeta {
	version: i32,
	libraries: Libraries,
	#[serde(rename = "mainClass")]
	main_class: MainClass,
}

#[derive(Serialize, Deserialize, Debug)]
struct LoaderMetaWithReleaseTime {
	meta: LoaderMeta,
	release_time: DateTime<Utc>,
}
