/*
 * Copyright 2022 kb1000
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::{collections::HashMap, fs, path::Path};

use anyhow::{bail, ensure, Context, Result};
use chrono::{DateTime, Utc};
use data_encoding::HEXLOWER;
use futures::{StreamExt, TryStreamExt};
use indexmap::{IndexMap, IndexSet};
use serde::Deserialize;
use serde_with::{serde_as, OneOrMany};
use sha1::{Digest, Sha1};

use helixlauncher_meta as helix;
use helixlauncher_meta::component::OsName;
use helixlauncher_meta::util::GradleSpecifier;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
enum VersionType {
	Experiment,
	Snapshot,
	Release,
	OldBeta,
	OldAlpha,
}

#[derive(Deserialize, Debug)]
struct VersionManifestVersion {
	pub id: String,
	pub url: String,
	pub sha1: String,
}

#[derive(Deserialize, Debug)]
struct VersionManifest {
	pub versions: Vec<VersionManifestVersion>,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RuleAction {
	Allow,
	Disallow,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct OsRule {
	name: Option<OsName>,
	version: Option<String>,
	arch: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct FeaturesRule {
	is_demo_user: Option<bool>,
	has_custom_resolution: Option<bool>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct Rule {
	features: Option<FeaturesRule>,
	os: Option<OsRule>,
	action: RuleAction,
}

impl Rule {
	fn is_always_allow(&self) -> bool {
		match self.action {
			RuleAction::Allow => self.features.is_none() && self.os.is_none(),
			_ => false,
		}
	}
}

#[serde_as]
#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum MojangConditionalValue<T> {
	Always(T),
	Conditional {
		rules: Vec<Rule>,
		#[serde_as(deserialize_as = "OneOrMany<_>")]
		value: Vec<T>,
	},
}

#[derive(Deserialize, Debug)]
struct MojangVersionArguments {
	game: Vec<MojangConditionalValue<String>>,
	jvm: Vec<MojangConditionalValue<String>>,
}

#[derive(Deserialize, Debug)]
struct MojangAssetIndex {
	id: String,
	sha1: String,
	size: u32,
	#[serde(rename = "totalSize")]
	total_size: u32,
	url: String,
}

impl From<MojangAssetIndex> for helix::component::Assets {
	fn from(assets: MojangAssetIndex) -> Self {
		Self {
			id: assets.id,
			url: assets.url,
			sha1: assets.sha1,
			size: assets.size,
			total_size: assets.total_size,
		}
	}
}

#[derive(Deserialize, Debug)]
struct MojangDownload {
	sha1: String,
	size: u32,
	url: String,
}

#[derive(Deserialize, Debug)]
struct MojangDownloads {
	client: MojangDownload,
}

#[derive(Deserialize, Debug)]
struct MojangJavaVersion {
	component: String,
	#[serde(rename = "majorVersion")]
	major_version: i32,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct MojangLibraryDownloads {
	artifact: Option<MojangLibraryArtifact>,
	#[serde(default)]
	classifiers: HashMap<String, MojangLibraryArtifact>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct MojangLibraryArtifact {
	path: String,
	sha1: String,
	size: u32,
	url: String,
}

#[derive(Deserialize, Default, Debug)]
struct MojangNativeExtract {
	exclude: Vec<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct MojangLibrary {
	name: GradleSpecifier,
	downloads: MojangLibraryDownloads,
	#[serde(default)]
	rules: Vec<Rule>,
	#[serde(default)]
	extract: MojangNativeExtract,
	#[serde(default)]
	natives: HashMap<OsName, String>,
}

#[derive(Deserialize, Debug)]
struct MojangLogging {}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct MojangVersion {
	arguments: Option<MojangVersionArguments>,
	asset_index: MojangAssetIndex,
	assets: String,
	_compliance_level: Option<i32>,
	downloads: MojangDownloads,
	id: String,
	java_version: Option<MojangJavaVersion>,
	libraries: Vec<MojangLibrary>,
	logging: Option<MojangLogging>,
	main_class: String,
	minecraft_arguments: Option<String>,
	_minimum_launcher_version: Option<i32>,
	release_time: DateTime<Utc>,
	time: DateTime<Utc>,
	#[serde(rename = "type")]
	version_type: VersionType,
}

mod rules {
	use super::{OsName, Rule, RuleAction};
	use thiserror::Error;

	#[derive(Error, Debug)]
	pub enum Error {
		#[error("Unsupported feature: {0}")]
		UnsupportedFeature(&'static str),
	}

	pub(super) fn evaluate_rules_os_name(rules: &[Rule]) -> Result<Vec<OsName>, Error> {
		let mut result = vec![];
		for current_os in [OsName::Linux, OsName::Osx, OsName::Windows] {
			let mut allow = false;
			for rule in rules {
				if let Some(os) = &rule.os {
					if os.arch.is_some() {
						return Err(Error::UnsupportedFeature("os.arch"));
					}
					if os.version.is_some() {
						return Err(Error::UnsupportedFeature("os.version"));
					}
					if let Some(osname) = os.name {
						if osname != current_os {
							continue;
						}
					}
				}
				if rule.features.is_some() {
					return Err(Error::UnsupportedFeature("features"));
				}
				allow = match rule.action {
					RuleAction::Allow => true,
					RuleAction::Disallow => false,
				}
			}
			if allow {
				result.push(current_os);
			}
		}
		Ok(result)
	}
}

const CONCURRENT_FETCH_LIMIT: Option<usize> = Some(5);

pub async fn fetch(client: &reqwest::Client) -> Result<()> {
	let version_base = Path::new("upstream/mojang/versions");
	fs::create_dir_all(version_base)?;

	let version_manifest: VersionManifest = client
		.get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
		.send()
		.await?
		.json()
		.await?;

	futures::stream::iter(version_manifest.versions)
		.map(Ok)
		.try_for_each_concurrent(CONCURRENT_FETCH_LIMIT, |v| async move {
			fetch_version(client, version_base, v).await
		})
		.await
}

async fn fetch_version(
	client: &reqwest::Client,
	version_base: &Path,
	version: VersionManifestVersion,
) -> Result<()> {
	let version_path = version_base.join(format!("{}.json", version.id));

	if version_path.try_exists()? {
		let content = fs::read(&version_path)?;
		if HEXLOWER.encode(&Sha1::digest(content)) == version.sha1 {
			return Ok(());
		}
	}
	let content = client.get(version.url).send().await?.bytes().await?;
	if HEXLOWER.encode(&Sha1::digest(&content)) != version.sha1 {
		bail!("{} has wrong SHA-1!", version.id)
	}
	fs::write(version_path, content)?;

	Ok(())
}

pub fn process() -> Result<()> {
	let version_base = Path::new("upstream/mojang/versions");
	let out_base = Path::new("out/net.minecraft");
	fs::create_dir_all(out_base)?;

	let mut index: helix::index::Index = vec![];

	for file in fs::read_dir(version_base)? {
		let file = file?;
		let component = process_version(&file, out_base)
			.with_context(|| format!("Failed to process {}", file.file_name().to_str().unwrap()))?;
		index.push(component.into());
	}

	fs::write(
		out_base.join("index.json"),
		serde_json::to_string_pretty(&index)?,
	)?;

	Ok(())
}

fn process_version(
	file: &fs::DirEntry,
	out_base: &Path,
) -> Result<helix::component::Component, anyhow::Error> {
	let version: MojangVersion = serde_json::from_str(&fs::read_to_string(file.path())?)
		.with_context(|| format!("Failed to parse {}", file.file_name().to_str().unwrap()))?;
	let mut classpath = IndexSet::with_capacity(version.libraries.len());
	let mut natives = IndexSet::with_capacity(version.libraries.len());
	let mut downloads = IndexMap::with_capacity(version.libraries.len() * 2);
	let game_download = version.downloads.client;
	let game_artifact_name = GradleSpecifier {
		group: "com.mojang".to_owned(),
		artifact: "minecraft".to_owned(),
		version: version.id.to_owned(),
		classifier: Some("client".to_owned()),
		extension: "jar".to_owned(),
	};
	downloads.insert(
		game_artifact_name.clone(),
		helix::component::Download {
			name: game_artifact_name.to_owned(),
			url: game_download.url,
			size: game_download.size,
			hash: helix::component::Hash::SHA1(game_download.sha1),
		},
	);
	let mut is_lwjgl3 = false;
	for library in version.libraries {
		let mut ignore_rules = false;
		ensure!(
			library.rules.len() <= 1
				|| (library.rules[0].is_always_allow() && library.rules.len() <= 2),
			"Multiple rules not handled currently"
		);
		if library.name.group.starts_with("org.lwjgl") {
			if library.name.version.starts_with("3.") {
				is_lwjgl3 = true;
			}

			// skip any LWJGL library specific to one OS (this might be too generic, but is fine
			// for everything currently existing)

			if library.rules.len() == 2
				&& library.rules[0].is_always_allow()
				&& matches!(library.rules[1].action, RuleAction::Disallow)
				&& matches!(&library.rules[1].os, Some(os) if os.name.is_some())
			{
				ignore_rules = true;
			}

			if library.rules.len() == 1
				&& matches!(library.rules[0].action, RuleAction::Allow)
				&& matches!(&library.rules[0].os, Some(os) if os.name.is_some())
				&& !matches!(&library.name.classifier, Some(classifier) if classifier.contains("natives"))
			{
				continue;
			}
		}

		let platform = if ignore_rules || library.rules.is_empty() {
			None
		} else {
			Some(helix::component::Platform {
				os: rules::evaluate_rules_os_name(&library.rules).with_context(|| {
					format!("Rules for \"{}\" failed to evaluate", library.name)
				})?,
				arch: None,
			})
		};

		let mut add_download = |name: &GradleSpecifier, artifact: &MojangLibraryArtifact| {
			if downloads.contains_key(name) {
				ensure!(
					matches!(&downloads[name].hash, helix::component::Hash::SHA1(sha1) if *sha1 == artifact.sha1)
				);
			} else {
				downloads.insert(
					name.to_owned(),
					helix::component::Download {
						name: name.to_owned(),
						url: artifact.url.to_owned(),
						size: artifact.size,
						hash: helix::component::Hash::SHA1(artifact.sha1.to_owned()),
					},
				);
			}
			Ok(())
		};

		if let Some(artifact) = library.downloads.artifact {
			add_download(&library.name, &artifact)?;
			classpath.insert(match &platform {
				None => helix::component::ConditionalClasspathEntry::All(library.name.to_owned()),
				Some(platform) => helix::component::ConditionalClasspathEntry::PlatformSpecific {
					name: library.name.to_owned(),
					platform: platform.clone(),
				},
			});
		}

		for (os, classifier) in library.natives {
			let mut process_native =
				|os: OsName, classifier: String, arch: Option<helix::component::Arch>| {
					ensure!(
						!classifier.contains('$'),
						"Unresolved classifier pattern in {}",
						classifier
					);
					let name = library.name.with_classifier(classifier.to_owned());
					add_download(
						&name,
						library
							.downloads
							.classifiers
							.get(&classifier)
							.with_context(|| {
								format!("{classifier} on {} does not exist", library.name)
							})?,
					)?;
					natives.insert(helix::component::Native {
						name,
						platform: helix::component::Platform { os: vec![os], arch },
						exclusions: library.extract.exclude.clone(),
					});
					Ok(())
				};
			if platform
				.as_ref()
				.map_or(true, |platform| platform.os.contains(&os))
			{
				if classifier.contains("${arch}") {
					process_native(
						os,
						classifier.replace("${arch}", "32"),
						Some(helix::component::Arch::X86),
					)?;
					process_native(
						os,
						classifier.replace("${arch}", "64"),
						Some(helix::component::Arch::X86_64),
					)?;
				} else {
					process_native(os, classifier, None)?;
				}
			}
		}
	}
	let component = helix::component::Component {
		format_version: 1,
		id: "net.minecraft".into(),
		traits: if is_lwjgl3 {
			vec![helix::component::Trait::MacStartOnFirstThread]
		} else {
			vec![]
		},
		assets: Some(version.asset_index.into()),
		version: version.id.to_owned(),
		requires: vec![], // TODO: lwjgl 2 (deal with that later)
		conflicts: vec![],
		downloads: downloads.into_values().collect(),
		classpath: classpath.into_iter().collect(),
		natives: natives.into_iter().collect(),
		main_class: Some(version.main_class),
		jarmods: vec![],
		game_jar: Some(game_artifact_name),
	};
	fs::write(
		out_base.join(format!("{}.json", version.id)),
		serde_json::to_string_pretty(&component)?,
	)?;
	Ok(component)
}
