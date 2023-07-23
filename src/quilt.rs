use std::{collections::BTreeSet, fs, path::Path, str::FromStr};

use anyhow::{Context, Result};
use chrono::{TimeZone, Utc};
use helixlauncher_meta::{
	component::{Component, ComponentDependency, ConditionalClasspathEntry, Download},
	index::Index, util::GradleSpecifier,
};
use reqwest::Client;
use serde::Deserialize;

use crate::{Metadata, Library};
pub async fn process(client: &Client) -> Result<()> {
	let out_base = Path::new("out/org.quiltmc.quilt-loader");
	fs::create_dir_all(out_base)?;

	let mut index: Index = vec![];

	for loader_version in get_loader_versions(client).await? {
		if loader_version == "0.17.5-beta.4" { // This version's meta is very broken and I hate it
			continue;
		}

		let response = client.get(format!("https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-loader/{loader_version}/quilt-loader-{loader_version}.json"))
            .header("User-Agent", "helixlauncher-meta (prototype)")
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
        let library = crate::Library { name: GradleSpecifier::from_str(&format!("org.quiltmc:quilt-loader:{loader_version}")).unwrap(), url: "https://maven.quiltmc.org/repository/release/".into() };
		let mut downloads = vec![Download {
            name: library.name.clone(),
            url: library.name.to_url(&library.url),
            hash: crate::get_hash(client, &library).await?,
            size: crate::get_size(client, &library).await?.try_into().unwrap(),
        }];
		let mut classpath = vec![ConditionalClasspathEntry::All(library.name)];
		for library in response.libraries.common {
			downloads.push(Download {
				name: library.name.clone(),
				url: library.name.to_url(&library.url),
				hash: crate::get_hash(client, &library).await?,
				size: crate::get_size(client, &library).await?.try_into().unwrap(),
			});
			classpath.push(ConditionalClasspathEntry::All(library.name))
		}

		let component = Component {
			format_version: 1,
			assets: None,
			conflicts: vec![],
			id: "org.quiltmc.quilt-loader".into(),
			jarmods: vec![],
			natives: vec![],
			release_time,
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
			main_class: Some(response.mainClass.client),
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
	let response = client.get("https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-loader/maven-metadata.xml")
        .header("User-Agent", "helixlauncher-meta (prototype)")
        .send().await?
        .text().await?;
	let response: Metadata = quick_xml::de::from_str(&response)?;
	Ok(response.versioning.versions.version)
}


#[derive(Deserialize, Debug)]
struct Libraries {
	client: Vec<Library>,
	common: Vec<Library>,
	server: Vec<Library>,
}

#[derive(Deserialize, Debug)]
struct MainClass {
	client: String,
	server: String,
	serverLauncher: Option<String>,
}

#[derive(Deserialize, Debug)]
struct LoaderMeta {
	version: i32,
	libraries: Libraries,
	mainClass: MainClass,
}
