use crate::support::jsons;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use simple_fs::SPath;
use std::collections::HashMap;
use std::fs::{self, OpenOptions, metadata};

// region:    --- Profiles Types

#[derive(Deserialize)]
struct ProfilesConfig {
	order: Vec<String>,
	#[serde(flatten)]
	profiles: HashMap<String, Profile>,
}

#[derive(Deserialize)]
struct Profile {
	zed_config: Vec<ZedConfigEntry>,
}

#[derive(Deserialize)]
struct ZedConfigEntry {
	config_path: Vec<String>,
	value: serde_json::Value,
}

#[derive(Deserialize, Serialize)]
struct CurrentProfile {
	current_profile: String,
}

// endregion: --- Profiles Types

/// Touches the Zed tasks.json file to trigger a reload if it exists.
/// NOTE: Needed because of a Zed bug (2026-01-09) that it does not refresh the current file in the environment variable.
///       The work around is to touch the zed tasks.json, and then, the current file
///       Now, since this binary is called after, we just touch_tasks_json for helping the next call. Not bullet proof, but should help.
pub fn touch_tasks_json() -> Result<()> {
	let home = home::home_dir().ok_or("Could not find home directory")?;
	let tasks_path = SPath::from_std_path(home)?.join(".config/zed/tasks.json");

	// with `filetime` crate (cleanest)
	// if tasks_path.exists() {
	// 	let now = FileTime::now();
	// 	set_file_times(&tasks_path, now, now)?;
	// }

	// A little hacky, but no filetime dep (might change later)
	if tasks_path.exists() {
		let len = metadata(&tasks_path)?.len();
		OpenOptions::new().write(true).open(tasks_path)?.set_len(len)?;
	}

	// With touch command
	// if tasks_path.exists() {
	// 	std::process::Command::new("touch").arg(tasks_path.as_str()).status()?;
	// }

	Ok(())
}

pub fn toggle_profile() -> Result<()> {
	let home = home::home_dir().ok_or("Could not find home directory")?;
	let config_dir = SPath::from_std_path(&home)?.join(".config/jc-zed-tasks");
	let profiles_path = config_dir.join("profiles.json");
	let current_path = config_dir.join("profile-current.json");
	let settings_path = SPath::from_std_path(&home)?.join(".config/zed/settings.json");

	if !profiles_path.exists() {
		return Err(Error::custom(format!("Profiles config not found at: {profiles_path}")));
	}

	if !settings_path.exists() {
		return Err(Error::custom(format!("Zed settings file not found at: {settings_path}")));
	}

	// -- Load configs
	let profiles_content = simple_fs::read_to_string(&profiles_path)?;
	let profiles_config: ProfilesConfig = serde_json::from_str(&profiles_content)?;

	if profiles_config.order.is_empty() {
		return Err(Error::custom("No profiles defined in 'order' array in profiles.json"));
	}

	let current_profile_name = if current_path.exists() {
		let current_content = simple_fs::read_to_string(&current_path)?;
		let current_config: CurrentProfile = serde_json::from_str(&current_content)?;
		current_config.current_profile
	} else {
		profiles_config.order[0].clone()
	};

	// -- Determine next profile
	let current_idx = profiles_config.order.iter().position(|p| p == &current_profile_name);
	let next_idx = match current_idx {
		Some(idx) => (idx + 1) % profiles_config.order.len(),
		None => 0,
	};
	let next_profile_name = &profiles_config.order[next_idx];
	let next_profile = profiles_config
		.profiles
		.get(next_profile_name)
		.ok_or_else(|| Error::custom(format!("Profile '{next_profile_name}' not found in profiles.json")))?;

	// -- Update settings.json
	let mut settings_content = simple_fs::read_to_string(&settings_path)?;
	for entry in &next_profile.zed_config {
		let path_refs: Vec<&str> = entry.config_path.iter().map(|s| s.as_str()).collect();
		settings_content = jsons::update_json_value_text_mode(&settings_content, &path_refs, &entry.value)?;
	}

	// -- Save changes
	fs::write(settings_path.std_path(), settings_content)?;
	let new_current = CurrentProfile { current_profile: next_profile_name.clone() };
	fs::write(current_path.std_path(), serde_json::to_string_pretty(&new_current)?)?;

	println!("Switched to profile: {next_profile_name}");
	touch_tasks_json()?;

	Ok(())
}
