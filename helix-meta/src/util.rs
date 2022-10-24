/*
 * Copyright 2022 kb1000
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::{fmt::Display, str::FromStr};

use serde_with::{DeserializeFromStr, SerializeDisplay};
use thiserror::Error;

#[derive(Debug, DeserializeFromStr, SerializeDisplay, Hash, Clone, PartialEq, Eq)]
pub struct GradleSpecifier {
	pub group: String,
	pub artifact: String,
	pub version: String,
	pub classifier: Option<String>,
	pub extension: String, // defaults to jar
}

impl GradleSpecifier {
	pub fn with_classifier(&self, classifier: String) -> Self {
		Self {
			classifier: Some(classifier),
			..self.clone()
		}
	}
}

#[derive(Error, Debug)]
pub enum GradleParseError {
	#[error("\"{0}\" does not contain an artifact id!")]
	ArtifactIdMissing(String),
	#[error("\"{0}\" does not contain a version!")]
	VersionMissing(String),
}

impl FromStr for GradleSpecifier {
	type Err = GradleParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let (group, s) = s
			.split_once(':')
			.ok_or_else(|| GradleParseError::ArtifactIdMissing(s.to_string()))?;
		let (artifact, s) = s
			.split_once(':')
			.ok_or_else(|| GradleParseError::VersionMissing(s.to_string()))?;
		let (s, extension) = s
			.rsplit_once('@')
			.map_or_else(|| (s, "jar"), |(s, extension)| (s, extension));
		let (version, classifier) = s.split_once(':').map_or_else(
			|| (s, None),
			|(version, classifier)| (version, Some(classifier)),
		);

		Ok(GradleSpecifier {
			group: group.to_owned(),
			artifact: artifact.to_owned(),
			version: version.to_owned(),
			classifier: classifier.map(|v| v.to_owned()),
			extension: extension.to_owned(),
		})
	}
}

impl Display for GradleSpecifier {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}:{}:{}", self.group, self.artifact, self.version)?;
		if let Some(classifier) = &self.classifier {
			write!(f, ":{}", classifier)?;
		}

		if self.extension != "jar" {
			write!(f, "@{}", self.extension)?;
		}
		Ok(())
	}
}
