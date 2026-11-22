use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::remote::{Remote, sftp};
use crate::util::expand_tilde;

const SAMPLE_CONFIG: &str = include_str!("../default_config.toml");
const AUTO_EXIT: u64 = 300;
const CONFIGFILE: &str = "~/.config/kptui/config.toml";
const KDF_ITERATIONS: u64 = 90;
pub const MIB: u64 = 1024 * 1024;
pub const DEFAULT_KDF_MEMORY: u64 = 256 * MIB;
const TMUX_CLEAR: u64 = 30;

fn sibling_tmp_path(path: &Path) -> PathBuf {
	let mut tmp = path.as_os_str().to_os_string();
	tmp.push(".tmp");
	PathBuf::from(tmp)
}

pub struct Config {
	/// The file kptui opens: for a remote database, its local copy.
	pub default_database: Option<PathBuf>,
	/// Set when the database is an `sftp://` URL.
	pub remote: Option<Remote>,
	pub keyfile: Option<PathBuf>,
	pub kdf_iterations: u64,
	pub kdf_memory: u64,
	pub auto_exit: Duration,
	pub theme: Option<String>,
	pub tmux_copy: bool,
	pub tmux_clear: Duration,
}

impl Default for Config {
	fn default() -> Self {
		Self {
			default_database: None,
			remote: None,
			keyfile: None,
			kdf_iterations: KDF_ITERATIONS,
			kdf_memory: DEFAULT_KDF_MEMORY,
			auto_exit: Duration::from_secs(AUTO_EXIT),
			theme: None,
			tmux_copy: false,
			tmux_clear: Duration::from_secs(TMUX_CLEAR),
		}
	}
}

impl Config {
	/// Sets the database from a path or an `sftp://` URL (empty: none).
	pub fn set_database(&mut self, value: &str) {
		let value = value.trim();
		if value.is_empty() {
			self.default_database = None;
			self.remote = None;
		} else if sftp::is_url(value) {
			let remote = Remote::new(value);
			self.default_database = Some(remote.cache_path());
			self.remote = Some(remote);
		} else {
			self.default_database = Some(expand_tilde(value));
			self.remote = None;
		}
	}

	/// The database as configured: its path, or its URL.
	pub fn database_label(&self) -> Option<String> {
		match &self.remote {
			Some(remote) => Some(remote.url().to_string()),
			None => self.default_database.as_ref().map(|path| path.display().to_string()),
		}
	}
}

#[derive(Deserialize, Default)]
struct ConfigFile {
	database: Option<String>,
	default_database: Option<String>,
	keyfile: Option<String>,
	kdf_iterations: Option<u64>,
	kdf_memory: Option<KdfMemoryValue>,
	auto_exit: Option<u64>,
	theme: Option<String>,
	tmux_copy: Option<bool>,
	tmux_clear: Option<u64>,
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum KdfMemoryValue {
	Bytes(u64),
	WithUnit(String),
}

impl KdfMemoryValue {
	fn into_bytes(self) -> anyhow::Result<u64> {
		match self {
			Self::Bytes(bytes) => Ok(bytes),
			Self::WithUnit(text) => {
				let value = text.trim().to_ascii_lowercase();
				let number = value
					.strip_suffix("mib")
					.or_else(|| value.strip_suffix("mb"))
					.or_else(|| value.strip_suffix('m'))
					.context(r#"kdf_memory requires a byte count or a quoted whole-MiB value such as "256M""#)?;
				let mib = number.trim().parse::<u64>().context("kdf_memory must contain a whole number of MiB")?;
				anyhow::ensure!(mib > 0, "kdf_memory must be at least 1 MiB");
				mib.checked_mul(MIB).context("kdf_memory is too large")
			}
		}
	}
}

pub fn load_config() -> anyhow::Result<Config> {
	let path = expand_tilde(CONFIGFILE);

	if !path.exists() {
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).with_context(|| format!("couldn't create {}", parent.display()))?;
		}

		let mut options = fs::OpenOptions::new();
		options.write(true).create_new(true);
		let result = options.open(&path);
		match result {
			Ok(mut file) => {
				use std::io::Write;
				file.write_all(SAMPLE_CONFIG.as_bytes()).with_context(|| format!("couldn't write {}", path.display()))?;
			}
			Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
			Err(error) => {
				return Err(error).with_context(|| format!("couldn't create {}", path.display()));
			}
		}
	}

	let text = fs::read_to_string(&path).with_context(|| format!("couldn't read {}", path.display()))?;
	parse_config(&text)
}

fn parse_config(text: &str) -> anyhow::Result<Config> {
	let raw: ConfigFile = toml::from_str(text)?;
	let defaults = Config::default();
	let mut config = Config {
		default_database: None,
		remote: None,
		keyfile: raw.keyfile.filter(|s| !s.is_empty()).map(|s| expand_tilde(&s)),
		kdf_iterations: raw.kdf_iterations.unwrap_or(defaults.kdf_iterations),
		kdf_memory: raw.kdf_memory.map(KdfMemoryValue::into_bytes).transpose()?.unwrap_or(defaults.kdf_memory),
		auto_exit: raw.auto_exit.map(Duration::from_secs).unwrap_or(defaults.auto_exit),
		theme: raw.theme,
		tmux_copy: raw.tmux_copy.unwrap_or(defaults.tmux_copy),
		tmux_clear: raw.tmux_clear.filter(|&secs| secs > 0).map(Duration::from_secs).unwrap_or(defaults.tmux_clear),
	};
	match raw.database.or(raw.default_database) {
		Some(database) if sftp::is_url(&database) => config.set_database(&database),
		database => config.default_database = database.map(|s| expand_tilde(&s)),
	}
	Ok(config)
}

#[derive(Serialize)]
struct ConfigFileOut<'a> {
	database: Option<String>,
	keyfile: String,
	kdf_iterations: u64,
	kdf_memory: KdfMemoryValue,
	auto_exit: u64,
	theme: Option<&'a str>,
	tmux_copy: bool,
	tmux_clear: u64,
}

pub fn save_config(config: &Config) -> anyhow::Result<()> {
	let path = expand_tilde(CONFIGFILE);

	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).with_context(|| format!("couldn't create {}", parent.display()))?;
	}

	let text = serialize_config(config)?;
	let tmp_path = sibling_tmp_path(&path);

	let mut options = fs::OpenOptions::new();
	options.write(true).create_new(true);

	let mut file = options.open(&tmp_path).with_context(|| format!("couldn't create {}", tmp_path.display()))?;

	let result = (|| {
		use std::io::Write;

		file.write_all(text.as_bytes()).with_context(|| format!("couldn't write {}", tmp_path.display()))?;

		file.sync_all().with_context(|| format!("couldn't sync {}", tmp_path.display()))?;

		fs::rename(&tmp_path, &path).with_context(|| format!("couldn't replace {}", path.display()))?;

		Ok(())
	})();

	if result.is_err() {
		let _ = fs::remove_file(&tmp_path);
	}

	result
}

fn serialize_config(config: &Config) -> anyhow::Result<String> {
	let out = ConfigFileOut {
		database: config.database_label(),
		keyfile: config.keyfile.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
		kdf_iterations: config.kdf_iterations,
		kdf_memory: if config.kdf_memory > 0 && config.kdf_memory % MIB == 0 {
			KdfMemoryValue::WithUnit(format!("{}M", config.kdf_memory / MIB))
		} else {
			KdfMemoryValue::Bytes(config.kdf_memory)
		},
		auto_exit: config.auto_exit.as_secs(),
		theme: config.theme.as_deref(),
		tmux_copy: config.tmux_copy,
		tmux_clear: config.tmux_clear.as_secs(),
	};

	toml::to_string_pretty(&out).context("couldn't serialize config")
}

#[cfg(test)]
mod tests {
	use super::{Config, parse_config, serialize_config};
	use std::path::PathBuf;
	use std::time::Duration;

	#[test]
	fn tmux_copy_is_off_by_default() {
		let config = parse_config("database = \"/tmp/passwords.kdbx\"").expect("config should parse");

		assert!(!config.tmux_copy);
		assert_eq!(config.tmux_clear, Duration::from_secs(30));
	}

	#[test]
	fn tmux_copy_settings_round_trip() {
		let config = parse_config(
			r#"
            tmux_copy = true
            tmux_clear = 5
        "#,
		)
		.expect("config should parse");

		assert!(config.tmux_copy);
		assert_eq!(config.tmux_clear, Duration::from_secs(5));

		let text = serialize_config(&config).expect("config should serialize");
		assert!(text.contains("tmux_copy = true"));
		assert!(text.contains("tmux_clear = 5"));
	}

	#[test]
	fn tmux_clear_of_zero_falls_back_to_the_default() {
		let config = parse_config("tmux_clear = 0").expect("config should parse");

		assert_eq!(config.tmux_clear, Duration::from_secs(30));
	}

	#[test]
	fn parses_optional_keyfile_path() {
		let config = parse_config(
			r#"
                database = "/tmp/passwords.kdbx"
                keyfile = "/tmp/passwords.keyx"
            "#,
		)
		.expect("config should parse");

		assert_eq!(config.default_database, Some(PathBuf::from("/tmp/passwords.kdbx")));
		assert_eq!(config.keyfile, Some(PathBuf::from("/tmp/passwords.keyx")));
	}

	#[test]
	fn keyfile_remains_optional() {
		let config = parse_config("database = \"/tmp/passwords.kdbx\"").expect("config should parse");

		assert_eq!(config.keyfile, None);

		let text = serialize_config(&config).expect("config should serialize");
		assert!(text.contains("keyfile = \"\""));
	}

	#[test]
	fn empty_keyfile_means_no_keyfile() {
		let config = parse_config(
			r#"
            database = "/tmp/passwords.kdbx"
            keyfile = ""
        "#,
		)
		.expect("config should parse");

		assert_eq!(config.keyfile, None);
	}

	#[test]
	fn serializes_keyfile_path() {
		let config = Config { keyfile: Some(PathBuf::from("/tmp/passwords.keyx")), ..Config::default() };

		let text = serialize_config(&config).expect("config should serialize");
		assert!(text.contains("keyfile = \"/tmp/passwords.keyx\""));
		assert!(!text.contains("database = "));
	}

	#[test]
	fn sftp_databases_open_a_local_copy_and_keep_their_url() {
		let url = "sftp://me@nas/srv/Sync%20Configs/vault.kdbx";
		let config = parse_config(&format!("database = \"{url}\"")).expect("config should parse");

		let remote = config.remote.as_ref().expect("the database should be remote");
		assert_eq!(remote.url(), url);
		assert_eq!(config.default_database, Some(remote.cache_path()));
		assert_eq!(config.database_label().as_deref(), Some(url));

		let text = serialize_config(&config).expect("config should serialize");
		assert!(text.contains(&format!("database = \"{url}\"")));
	}

	#[test]
	fn setting_a_local_database_drops_the_remote_one() {
		let mut config = Config::default();
		config.set_database("sftp://nas/vault.kdbx");
		assert!(config.remote.is_some());

		config.set_database("/tmp/passwords.kdbx");
		assert!(config.remote.is_none());
		assert_eq!(config.database_label().as_deref(), Some("/tmp/passwords.kdbx"));

		config.set_database("  ");
		assert_eq!(config.default_database, None);
		assert_eq!(config.database_label(), None);
	}

	#[test]
	fn parses_legacy_default_database_key() {
		let config = parse_config(r#"default_database = "/tmp/passwords.kdbx""#).expect("legacy config should parse");

		assert_eq!(config.default_database, Some(PathBuf::from("/tmp/passwords.kdbx")));
	}

	#[test]
	fn database_key_takes_precedence_over_legacy_key() {
		let config = parse_config(
			r#"
            database = "/tmp/new.kdbx"
            default_database = "/tmp/old.kdbx"
        "#,
		)
		.expect("config should parse");

		assert_eq!(config.default_database, Some(PathBuf::from("/tmp/new.kdbx")));
	}
	#[test]
	fn memory_defaults_and_legacy_values_round_trip() {
		assert_eq!(parse_config("").unwrap().kdf_memory, 256 * super::MIB);
		for bytes in [262_144, 268_435_456] {
			let text = format!("kdf_memory = {bytes}");
			let config = parse_config(&text).unwrap();
			assert_eq!(config.kdf_memory, bytes);
			let saved = serialize_config(&config).unwrap();
			assert_eq!(parse_config(&saved).unwrap().kdf_memory, bytes);
		}
	}
	#[test]
	fn memory_accepts_mib_suffixes_and_saves_the_short_form() {
		for text in ["256m", "256M", "256MB", "256MiB", "256mb", "256mib"] {
			let config = parse_config(&format!("kdf_memory = {text:?}")).unwrap();
			assert_eq!(config.kdf_memory, 256 * super::MIB);
			let saved = serialize_config(&config).unwrap();
			assert!(saved.contains(r#"kdf_memory = "256M""#));
			assert_eq!(parse_config(&saved).unwrap().kdf_memory, config.kdf_memory);
		}
		let config = parse_config("kdf_memory = 268435456").unwrap();
		assert!(serialize_config(&config).unwrap().contains(r#"kdf_memory = "256M""#));
		let legacy = parse_config("kdf_memory = 262144").unwrap();
		assert!(serialize_config(&legacy).unwrap().contains("kdf_memory = 262144"));
	}

	#[test]
	fn memory_rejects_invalid_units_fractions_and_overflow() {
		for text in ["0M", "-1M", "1.5M", "256GB", "256", "18446744073709551615M", ""] {
			assert!(parse_config(&format!("kdf_memory = {text:?}")).is_err(), "{text}");
		}
	}
}
