use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::component;

pub type Index = Vec<IndexEntry>;

#[derive(Serialize, Deserialize, Debug)]
pub struct IndexEntry {
	pub version: String,
	pub release_time: DateTime<Utc>,
	#[serde(skip_serializing_if = "Vec::is_empty", default)]
	pub conflicts: Vec<component::ComponentDependency>,
	#[serde(skip_serializing_if = "Vec::is_empty", default)]
	pub requires: Vec<component::ComponentDependency>,
}

impl From<&component::Component> for IndexEntry {
	fn from(component: &component::Component) -> Self {
		Self {
			version: component.version.to_string(),
			conflicts: component.conflicts.to_vec(),
			requires: component.requires.to_vec(),
			release_time: component.release_time,
		}
	}
}
impl From<component::Component> for IndexEntry {
	fn from(component: component::Component) -> Self {
		Self {
			version: component.version,
			conflicts: component.conflicts,
			requires: component.requires,
			release_time: component.release_time,
		}
	}
}
