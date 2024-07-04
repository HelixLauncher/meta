/*
 * Copyright 2022-2023 kb1000
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{ensure, Context, Result};

use helixlauncher_meta as helix;
use lazy_static::lazy_static;
use regex::Regex;

use crate::mojang;

pub fn process() -> Result<()> {
	let version_base = Path::new("upstream/forge/installers");
	fs::create_dir_all(version_base)?;
	let out_base = Path::new("out/net.minecraftforge.forge");
	fs::create_dir_all(out_base)?;

	let mut index: helix::index::Index = vec![];

	for file in fs::read_dir(version_base)? {
		let file = file?;
		let component = process_version(&file, out_base)
			.with_context(|| format!("Failed to process {}", file.file_name().to_str().unwrap()))?;
		index.push(component.into());
	}

	index.sort_by(|x, y| y.release_time.cmp(&x.release_time));

	fs::write(
		out_base.join("index.json"),
		serde_json::to_string_pretty(&index)?,
	)?;

	Ok(())
}

fn process_version(file: &fs::DirEntry, out_base: &Path) -> Result<helix::component::Component> {
	// FIXME: this doesn't support like anything other than 1.12.2 and some more recent older versions
	lazy_static! {
		static ref VERSION_PATTERN: Regex =
			Regex::new("^(?:[0-9.]+-forge-|[0-9.]+-Forge)(?P<forge_version>[0-9.]+)$").unwrap();
	}
	let mut archive = zip::ZipArchive::new(std::fs::File::open(file.path())?)?;

	let file = std::io::BufReader::new(archive.by_name("version.json")?);
	let version: mojang::MojangVersion = serde_json::from_reader(file)?;
	ensure!(version.downloads.is_none());
	ensure!(version.asset_index.is_none());
	ensure!(version.arguments.is_none());
	let arguments = version
		.minecraft_arguments
		.with_context(|| "Minecraft arguments missing")?;
	let minecraft_version = version
		.inherits_from
		.with_context(|| "Minecraft version missing")?;
	let m = VERSION_PATTERN
		.captures(&version.id)
		.with_context(|| format!("Could not extract Forge version from {}", version.id))?;
	let forge_version = m.name("forge_version").unwrap().as_str();
	let mut downloads = Vec::with_capacity(version.libraries.len());
	let mut classpath = Vec::with_capacity(version.libraries.len());
	for library in version.libraries {
		ensure!(library.rules.is_empty());
		ensure!(library.natives.is_empty());
		ensure!(library.downloads.classifiers.is_empty());
		let artifact = library
			.downloads
			.artifact
			.with_context(|| format!("Artifact for {} missing", library.name))?;
		downloads.push(helix::component::Download {
			name: library.name.clone(),
			url: artifact.url,
			size: artifact.size,
			hash: helix::component::Hash::SHA1(artifact.sha1),
		});
		classpath.push(helix::component::ConditionalClasspathEntry::All(
			library.name,
		));
	}
	let args = &arguments[arguments
		.find("--tweakClass")
		.with_context(|| "Invalid Minecraft arguments")?..];
	ensure!(!args.contains('$'));
	let component = helix::component::Component {
		format_version: 1,
		id: "net.minecraftforge.forge".into(),
		version: forge_version.into(),
		requires: vec![helix::component::ComponentDependency {
			id: "net.minecraft".into(),
			version: Some(minecraft_version),
		}],
		traits: BTreeSet::new(),
		assets: None,
		conflicts: vec![],
		downloads,
		jarmods: vec![],
		game_jar: None,
		main_class: Some(version.main_class),
		game_arguments: args
			.split(' ')
			.map(|s| helix::component::MinecraftArgument::Always(s.into()))
			.collect(),
		classpath,
		natives: vec![],
		release_time: version.release_time,
	};
	fs::write(
		out_base.join(format!("{}.json", component.version)),
		serde_json::to_string_pretty(&component)?,
	)?;
	Ok(component)
}
