//! Transfers through the system's OpenSSH `sftp` client.
//!
//! Using OpenSSH gives kptui the user's `~/.ssh/config`, keys, agent and
//! `known_hosts` checking, without an SSH implementation of its own. `sftp`
//! runs in batch mode, so it never prompts: logging in has to work without a
//! password (key or agent), and the host key must be known already. Only the
//! encrypted database file is ever transferred.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const SCHEME: &str = "sftp://";
const CONNECT_TIMEOUT: &str = "ConnectTimeout=8";
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(120);
const REASON_MAX: usize = 80;

/// Whether a configured database is an `sftp://` URL rather than a local path.
pub fn is_url(value: &str) -> bool {
	value.trim_start().get(..SCHEME.len()).is_some_and(|scheme| scheme.eq_ignore_ascii_case(SCHEME))
}

/// Why a transfer didn't happen.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
	/// Couldn't connect or log in.
	Unreachable(String),
	/// Connected, but there is no such file.
	Missing,
	/// Connected, but the transfer failed.
	Failed(String),
	/// Broken off, because it's time for auto-exit.
	Stopped,
}

impl std::fmt::Display for Failure {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Unreachable(reason) => write!(f, "server unreachable: {}", shorten(reason)),
			Self::Missing => f.write_str("not found on the server"),
			Self::Failed(reason) => f.write_str(&shorten(reason)),
			Self::Stopped => f.write_str("stopped by auto-exit"),
		}
	}
}

impl std::error::Error for Failure {}

/// A database file on an SFTP server: `sftp://[user@]host[:port]/path`.
///
/// The host can be an alias from `~/.ssh/config`. The path is absolute, or
/// relative to the login directory when it starts with `/~/`. It may be
/// %-encoded, like `%20` for a space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
	/// `[user@]host`, as given to `sftp`.
	destination: String,
	host: String,
	port: Option<u16>,
	/// Absolute, or starting with `./` for the login directory.
	path: String,
}

/// What to replace the file on the server with, see `Target::replace`.
pub struct Upload<'a> {
	pub name: &'a str,
	/// Where to download what gets replaced.
	pub replaced: &'a str,
	/// Whether there must be a file to replace, or must not be one.
	pub must_exist: bool,
	/// The permissions to set, to keep those of the replaced file.
	pub mode: Option<u32>,
	/// Identifies this device.
	pub id: &'a str,
}

impl Target {
	pub fn parse(url: &str) -> Result<Self, String> {
		let url = url.trim();
		if !is_url(url) {
			return Err("it doesn't start with sftp://".to_string());
		}
		let (authority, path) = url[SCHEME.len()..].split_once('/').ok_or("there is no path to the database file")?;
		let (user, host_port) = match authority.rsplit_once('@') {
			Some((user, host_port)) => (Some(user), host_port),
			None => (None, authority),
		};
		let (host, port) = split_port(host_port)?;
		check_host(host)?;
		let destination = match user {
			Some(user) => {
				check_user(user)?;
				format!("{user}@{host}")
			}
			None => host.to_string(),
		};

		let path = percent_decode(path)?;
		let path = match path.strip_prefix("~/") {
			Some(relative) => format!("./{relative}"),
			None => format!("/{path}"),
		};
		check_path(&path)?;

		Ok(Self { destination, host: host.trim_start_matches('[').trim_end_matches(']').to_string(), port, path })
	}

	pub fn host(&self) -> &str {
		&self.host
	}

	pub fn file_name(&self) -> &str {
		self.path.rsplit('/').next().unwrap_or(&self.path)
	}

	/// Downloads the file to `dir/name`, keeping its permissions.
	pub fn download(&self, dir: &Path, name: &str, until: Option<Instant>) -> Result<(), Failure> {
		let path = dir.join(name);
		let _ = fs::remove_file(&path);
		match self.run(dir, &format!("get -p {} {}\n", escape(&self.path), escape(name)), until) {
			Ok(()) => File::open(&path).and_then(|file| file.sync_all()).map_err(|err| Failure::Failed(format!("couldn't store it: {err}"))),
			Err(Failure::Failed(reason)) if is_missing(&reason) => Err(Failure::Missing),
			result => result,
		}
	}

	/// Replaces the file with `upload.name`. It goes to a hidden file next to it
	/// first, which then gets renamed over it, so the file is replaced in one go
	/// (this needs OpenSSH's posix-rename, which OpenSSH servers have).
	///
	/// SFTP can't replace a file only if it didn't change. So right before the
	/// rename, in the same session, what's about to be replaced is downloaded to
	/// `upload.replaced`, to notice a change and keep it. Without `must_exist`,
	/// this fails if the file does exist (a rename without posix-rename does).
	pub fn replace(&self, dir: &Path, upload: &Upload, until: Option<Instant>) -> Result<(), Failure> {
		let _ = fs::remove_file(dir.join(upload.replaced));
		let staging = escape(&self.staging_path(upload.id));
		let path = escape(&self.path);
		let mut commands = format!("-rm {staging}\nput {} {staging}\n", escape(upload.name));
		if let Some(mode) = upload.mode {
			commands.push_str(&format!("-chmod {:o} {staging}\n", mode & 0o777));
		}
		commands.push_str(&if upload.must_exist {
			// Not being able to download it stops the batch, before the rename.
			format!("get -p {path} {}\nrename {staging} {path}\n", escape(upload.replaced))
		} else {
			format!("rename -l {staging} {path}\n")
		});

		let result = self.run(dir, &commands, until);
		if let Err(Failure::Failed(reason)) = &result {
			let _ = self.run(dir, &format!("-rm {staging}\n"), until);
			if upload.must_exist && is_missing(reason) {
				return Err(Failure::Missing);
			}
		}
		result
	}

	/// Every device has its own hidden file, so uploads at the same time don't mix.
	fn staging_path(&self, id: &str) -> String {
		match self.path.rsplit_once('/') {
			Some((dir, file)) => format!("{dir}/.{file}.kptui-upload-{id}"),
			None => format!(".{}.kptui-upload-{id}", self.path),
		}
	}

	/// Runs `sftp` with `commands` as its batch file, in `dir`. It's broken off
	/// after a while, and at `until`.
	fn run(&self, dir: &Path, commands: &str, until: Option<Instant>) -> Result<(), Failure> {
		let stop = || until.is_some_and(|until| Instant::now() >= until);
		if stop() {
			return Err(Failure::Stopped);
		}
		let mut command = Command::new("sftp");
		command.args(["-b", "-", "-o", "BatchMode=yes", "-o", CONNECT_TIMEOUT, "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=3"]);
		if let Some(port) = self.port {
			command.arg("-P").arg(port.to_string());
		}
		command
			.arg("--")
			.arg(&self.destination)
			.current_dir(dir)
			.stdin(Stdio::piped())
			.stdout(Stdio::null())
			.stderr(Stdio::piped());

		let mut child = command.spawn().map_err(|err| Failure::Unreachable(format!("couldn't run sftp: {err}")))?;
		// The commands are tiny, so writing them can't block. Closing stdin ends the batch.
		if let Some(mut stdin) = child.stdin.take() {
			let _ = stdin.write_all(commands.as_bytes());
		}

		let (sender, receiver) = mpsc::channel();
		if let Some(mut stderr) = child.stderr.take() {
			std::thread::spawn(move || {
				let mut text = String::new();
				let _ = stderr.read_to_string(&mut text);
				let _ = sender.send(text);
			});
		}

		let deadline = Instant::now() + TRANSFER_TIMEOUT;
		let status = loop {
			match child.try_wait() {
				Ok(Some(status)) => break Some(status),
				Ok(None) if Instant::now() < deadline && !stop() => std::thread::sleep(Duration::from_millis(50)),
				_ => {
					let _ = child.kill();
					let _ = child.wait();
					break None;
				}
			}
		};
		// A process that inherited stderr could keep it open, so don't wait long for it.
		let stderr = receiver.recv_timeout(Duration::from_secs(1)).unwrap_or_default();

		match status {
			Some(status) if status.success() => Ok(()),
			// ssh exits with 255 when it couldn't connect or log in, or lost the connection.
			Some(status) if status.code().is_none_or(|code| code == 255) => Err(Failure::Unreachable(reason(&stderr, "couldn't connect"))),
			Some(_) => Err(Failure::Failed(reason(&stderr, "the transfer failed"))),
			None if stop() => Err(Failure::Stopped),
			None => Err(Failure::Unreachable("timed out".to_string())),
		}
	}
}

/// The last useful line `sftp` printed, which says what went wrong.
fn reason(stderr: &str, fallback: &str) -> String {
	let line = stderr
		.lines()
		.map(str::trim)
		.rfind(|line| !line.is_empty() && !line.starts_with("Connection closed") && !line.starts_with("Warning: Permanently added"))
		.unwrap_or(fallback);
	line.strip_prefix("ssh: ").unwrap_or(line).to_string()
}

fn shorten(reason: &str) -> String {
	match reason.char_indices().nth(REASON_MAX) {
		Some((cut, _)) => format!("{}...", &reason[..cut]),
		None => reason.to_string(),
	}
}

/// Whether the reason is that there's no such file on the server.
fn is_missing(reason: &str) -> bool {
	!reason.starts_with("open local") && (reason.ends_with("\" not found.") || reason.ends_with("No such file or directory"))
}

/// Escapes a path for an `sftp` batch file.
fn escape(path: &str) -> String {
	let mut out = String::with_capacity(path.len() + 8);
	for c in path.chars() {
		if matches!(c, '\\' | '"' | '\'' | ' ' | '#') {
			out.push('\\');
		}
		out.push(c);
	}
	out
}

fn split_port(host_port: &str) -> Result<(&str, Option<u16>), String> {
	let (host, port) = if host_port.starts_with('[') {
		let end = host_port.find(']').ok_or("there is an unclosed [ in the host")?;
		match &host_port[end + 1..] {
			"" => (&host_port[..=end], None),
			rest => (&host_port[..=end], Some(rest.strip_prefix(':').ok_or("there is text after ] in the host")?)),
		}
	} else {
		match host_port.split_once(':') {
			Some((host, port)) => (host, Some(port)),
			None => (host_port, None),
		}
	};
	let port = match port {
		Some(port) => Some(port.parse::<u16>().ok().filter(|&port| port > 0).ok_or_else(|| format!("\"{port}\" isn't a port number"))?),
		None => None,
	};
	Ok((host, port))
}

fn check_host(host: &str) -> Result<(), String> {
	let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_');
	let valid = match host.strip_prefix('[').and_then(|inner| inner.strip_suffix(']')) {
		Some(ipv6) => !ipv6.is_empty() && ipv6.chars().all(|c| c.is_ascii_hexdigit() || matches!(c, ':' | '.')),
		None => !host.is_empty() && !host.starts_with('-') && host.chars().all(plain),
	};
	if valid { Ok(()) } else { Err(format!("\"{host}\" isn't a valid host")) }
}

fn check_user(user: &str) -> Result<(), String> {
	if user.contains(':') {
		return Err("passwords in the URL aren't supported, use a key or ssh-agent".to_string());
	}
	if user.is_empty() || user.starts_with('-') || user.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '@' | '/')) {
		return Err(format!("\"{user}\" isn't a valid user name"));
	}
	Ok(())
}

fn check_path(path: &str) -> Result<(), String> {
	if path.chars().any(char::is_control) {
		return Err("the path contains control characters".to_string());
	}
	// sftp treats these as wildcards in some commands.
	if path.contains(['*', '?', '[', ']']) {
		return Err("the path can't contain * ? [ or ]".to_string());
	}
	if path.ends_with('/') || path.rsplit('/').next().is_none_or(|name| name.is_empty() || name == "." || name == "..") {
		return Err("the path doesn't end in a file name".to_string());
	}
	Ok(())
}

fn percent_decode(text: &str) -> Result<String, String> {
	let bytes = text.as_bytes();
	let mut out = Vec::with_capacity(bytes.len());
	let mut i = 0;
	while i < bytes.len() {
		if bytes[i] == b'%' {
			let hex = bytes
				.get(i + 1..i + 3)
				.filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))
				.ok_or("there is an invalid %-escape in the path")?;
			// Two ASCII hex digits always parse.
			out.push(u8::from_str_radix(std::str::from_utf8(hex).unwrap_or("00"), 16).unwrap_or(0));
			i += 3;
		} else {
			out.push(bytes[i]);
			i += 1;
		}
	}
	String::from_utf8(out).map_err(|_| "the path isn't valid UTF-8".to_string())
}

#[cfg(test)]
mod tests {
	use super::{Failure, REASON_MAX, Target, escape, is_missing, is_url, reason};

	fn parse(url: &str) -> Target {
		Target::parse(url).unwrap_or_else(|err| panic!("{url} should parse: {err}"))
	}

	#[test]
	fn recognizes_sftp_urls() {
		assert!(is_url("sftp://nas/vault.kdbx"));
		assert!(is_url("  SFTP://nas/vault.kdbx"));
		assert!(!is_url("/home/me/vault.kdbx"));
		assert!(!is_url("~/sftp://x"));
	}

	#[test]
	fn parses_user_host_port_and_paths() {
		let target = parse("sftp://me@example.com:2222/srv/vaults/My%20Vault.kdbx");
		assert_eq!(target.destination, "me@example.com");
		assert_eq!(target.host(), "example.com");
		assert_eq!(target.port, Some(2222));
		assert_eq!(target.path, "/srv/vaults/My Vault.kdbx");
		assert_eq!(target.file_name(), "My Vault.kdbx");

		let target = parse("sftp://nas/~/Sync Configs/Database.kdbx");
		assert_eq!(target.destination, "nas");
		assert_eq!(target.port, None);
		assert_eq!(target.path, "./Sync Configs/Database.kdbx");

		let target = parse("sftp://me@[2001:db8::1]:22/vault.kdbx");
		assert_eq!(target.destination, "me@[2001:db8::1]");
		assert_eq!(target.host(), "2001:db8::1");
		assert_eq!(target.port, Some(22));
		assert_eq!(target.path, "/vault.kdbx");
	}

	#[test]
	fn rejects_invalid_urls() {
		for url in [
			"sftp://nas",
			"sftp:///vault.kdbx",
			"sftp://-oProxyCommand=x/vault.kdbx",
			"sftp://me:secret@nas/vault.kdbx",
			"sftp://nas:0/vault.kdbx",
			"sftp://nas:ssh/vault.kdbx",
			"sftp://nas/vaults/",
			"sftp://nas/~/",
			"sftp://nas/vault*.kdbx",
			"sftp://nas/vault%0a.kdbx",
			"sftp://nas/vault%zz.kdbx",
			"sftp://[::1/vault.kdbx",
		] {
			assert!(Target::parse(url).is_err(), "{url} should be rejected");
		}
	}

	#[test]
	fn upload_goes_to_a_hidden_file_next_to_the_target() {
		assert_eq!(parse("sftp://nas/srv/vault.kdbx").staging_path("12ab"), "/srv/.vault.kdbx.kptui-upload-12ab");
		assert_eq!(parse("sftp://nas/vault.kdbx").staging_path("12ab"), "/.vault.kdbx.kptui-upload-12ab");
		assert_eq!(parse("sftp://nas/~/vault.kdbx").staging_path("12ab"), "./.vault.kdbx.kptui-upload-12ab");
	}

	#[test]
	fn nothing_runs_once_it_is_time_for_auto_exit() {
		let target = parse("sftp://nas.invalid/vault.kdbx");
		assert_eq!(target.download(&std::env::temp_dir(), "kptui-never-downloaded", Some(std::time::Instant::now())), Err(Failure::Stopped));
	}

	#[test]
	fn escapes_paths_for_batch_files() {
		assert_eq!(escape("/a b/c\"d'e\\f#g$h"), "/a\\ b/c\\\"d\\'e\\\\f\\#g$h");
	}

	#[test]
	fn reports_the_last_useful_line() {
		let stderr = "Load key \"x\": invalid format\nme@nas: Permission denied (publickey).\nConnection closed\n";
		assert_eq!(reason(stderr, "fallback"), "me@nas: Permission denied (publickey).");
		assert_eq!(
			reason("ssh: connect to host nas port 22: Connection refused\nConnection closed.\n", "x"),
			"connect to host nas port 22: Connection refused"
		);
		assert_eq!(reason("", "fallback"), "fallback");

		let long = Failure::Failed("x".repeat(200));
		assert_eq!(long.to_string().len(), REASON_MAX + 3);
	}

	#[test]
	fn knows_when_there_is_no_such_file() {
		// Also when the path is longer than what's shown.
		let path = "/long".repeat(40);
		for line in [format!("File \"{path}\" not found."), format!("remote open \"{path}\": No such file or directory")] {
			assert!(is_missing(&reason(&line, "")), "{line}");
		}
		for line in [
			"remote open \"/not found./vault.kdbx\": Permission denied",
			"open local \"vault.kdbx.download\": No such file or directory",
			"Failure",
		] {
			assert!(!is_missing(&reason(line, "")), "{line}");
		}
	}
}
