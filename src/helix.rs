/*
 * Copyright 2022 kb1000
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use serde::Serialize;
use serde_with::{serde_as, skip_serializing_none, OneOrMany};

use crate::{mojang::MojangOsName, util::GradleSpecifier};

#[derive(Serialize)]
pub struct ComponentDependency {
	pub id: String,
	#[serde(skip_serializing_if = "Option::is_none", default)]
	pub version: Option<String>,
}

#[derive(Serialize)]
pub struct Download {
	pub name: GradleSpecifier,
	pub url: String,
	// these two might have to be made optional
	pub size: i32,
	pub sha1: String,
}

#[derive(Serialize)]
pub enum Trait {
	MacStartOnFirstThread,
}

#[derive(Serialize, Hash, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Arch {
	X86,
	X86_64,
	Arm64,
}

#[serde_as]
#[derive(Serialize, Hash, PartialEq, Eq, Clone)]
pub struct Platform {
	#[serde(skip_serializing_if = "Vec::is_empty", default)]
	#[serde_as(as = "OneOrMany<_>")]
	pub os: Vec<MojangOsName>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub arch: Option<Arch>,
}

#[derive(Serialize, Hash, PartialEq, Eq, Clone)]
pub struct Native {
	pub name: GradleSpecifier,
	pub platform: Platform,
	#[serde(skip_serializing_if = "Vec::is_empty", default)]
	pub exclusions: Vec<String>,
}

#[derive(Serialize, Hash, PartialEq, Eq)]
#[serde(untagged)]
pub enum ConditionalClasspathEntry {
	All(GradleSpecifier),
	PlatformSpecific {
		name: GradleSpecifier,
		platform: Platform,
	},
}

#[skip_serializing_none]
#[derive(Serialize)]
pub struct Component {
	pub format_version: i32,
	pub id: String,
	pub version: String,
	#[serde(skip_serializing_if = "Vec::is_empty", default)]
	pub requires: Vec<ComponentDependency>,
	#[serde(skip_serializing_if = "Vec::is_empty", default)]
	pub traits: Vec<Trait>,
	#[serde(skip_serializing_if = "Vec::is_empty", default)]
	pub conflicts: Vec<ComponentDependency>,
	pub downloads: Vec<Download>,
	pub classpath: Vec<ConditionalClasspathEntry>,
	#[serde(skip_serializing_if = "Vec::is_empty", default)]
	pub natives: Vec<Native>,
	pub main_class: Option<String>,
	pub game_jar: Option<GradleSpecifier>, // separate from classpath to make injecting jarmods possible
}
