//! Databases on an SFTP server, used through a local copy (like KeePass2Android).
//!
//! kptui opens and saves the local copy, and this module keeps it in sync:
//! - The local copy is locked, so only one kptui on this device uses it.
//! - When the database gets selected, the server copy replaces the local one,
//!   unless that has changes that weren't uploaded yet. If the server can't be
//!   reached, the local copy gets used and changes are uploaded later.
//! - After every change, the local copy gets uploaded, if the server copy is
//!   still the one it started from. If not, both get merged first (see `merge`).
//! - Next to the local copy, the last version known to be on the server is kept
//!   (the "base"). Comparing it with the local copy tells whether there are
//!   changes to upload, and with a fresh download whether the server copy
//!   changed. It's also what both sides started from, for merging.
//! - Another device can save during an upload, and SFTP can't prevent replacing
//!   that. So what gets replaced is downloaded right before, in the same session:
//!   if it's not what was expected, it gets merged and uploaded, too. Only a
//!   change in the moment between those two commands goes unnoticed.
//! - The files are written in such an order that when kptui gets killed, the next
//!   sync finds what wasn't uploaded or merged.
//! - Just opening a database never writes to the server, only changes do.

pub mod merge;
pub mod sftp;

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::hash::{BuildHasher, Hasher, RandomState};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use anyhow::{Context, bail, ensure};
use keepass::Database;

use crate::app::{App, Screen};
use crate::util::expand_tilde;
use sftp::{Failure, Target, Upload};

const DIR: &str = "~/.local/share/kptui/remote";
const UPLOADING: &str = ", uploading...";
/// How often the server copy may change during an upload, before giving up.
const ROUNDS: usize = 5;

/// A database configured as an `sftp://` URL.
pub struct Remote {
	url: String,
	target: Result<Target, String>,
}

impl Remote {
	pub fn new(url: &str) -> Self {
		let url = url.trim().to_string();
		let target = Target::parse(&url);
		Self { url, target }
	}

	pub fn url(&self) -> &str {
		&self.url
	}

	fn target(&self) -> Result<Target, Failure> {
		self.target.clone().map_err(|err| Failure::Failed(format!("invalid sftp URL: {err}")))
	}

	/// The local copy, which kptui opens and saves.
	pub fn cache_path(&self) -> PathBuf {
		let label = match &self.target {
			Ok(target) => format!("{}-{}", target.host(), target.file_name().trim_end_matches(".kdbx")),
			Err(_) => "invalid".to_string(),
		};
		let label: String = label
			.chars()
			.map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
			.take(64)
			.collect();
		expand_tilde(DIR).join(format!("{label}-{:016x}.kdbx", fnv1a(self.url.as_bytes())))
	}

	fn files(&self) -> Files {
		let cache = self.cache_path();
		Files {
			base: with_suffix(&cache, ".base"),
			download: with_suffix(&cache, ".download"),
			uploading: with_suffix(&cache, ".uploading"),
			replaced: with_suffix(&cache, ".replaced"),
			lock: with_suffix(&cache, ".lock"),
			id: with_suffix(&cache, ".id"),
			cache,
		}
	}
}

/// The files kept for a remote database.
struct Files {
	/// The local copy.
	cache: PathBuf,
	/// The last version known to be on the server.
	base: PathBuf,
	/// A download of the server copy.
	download: PathBuf,
	/// What's being uploaded, until it's known to be on the server.
	uploading: PathBuf,
	/// What that replaces on the server, until it's known to be merged.
	replaced: PathBuf,
	/// Locked while a kptui uses the local copy.
	lock: PathBuf,
	/// Identifies this device in the name of uploads.
	id: PathBuf,
}

impl Files {
	fn dir(&self) -> &Path {
		self.cache.parent().unwrap_or(Path::new("."))
	}

	/// Whether there are changes that aren't on the server, or weren't merged yet.
	fn unsynced(&self) -> bool {
		self.replaced.is_file() || !same_contents(&self.cache, &self.base)
	}

	/// Takes the lock on the local copy, or `None` when another kptui has it.
	/// It's released when the file gets closed, however kptui ends.
	fn lock(&self) -> anyhow::Result<Option<File>> {
		create_dir(self.dir()).with_context(|| format!("couldn't create {}", self.dir().display()))?;
		let mut options = OpenOptions::new();
		options.write(true).create(true);
		#[cfg(unix)]
		{
			use std::os::unix::fs::OpenOptionsExt;
			options.mode(0o600);
		}
		let file = options.open(&self.lock).with_context(|| format!("couldn't create {}", self.lock.display()))?;
		match file.try_lock() {
			Ok(()) => Ok(Some(file)),
			Err(TryLockError::WouldBlock) => Ok(None),
			Err(TryLockError::Error(err)) => Err(err).context("couldn't lock the local copy"),
		}
	}

	fn device_id(&self) -> String {
		if let Ok(id) = fs::read_to_string(&self.id)
			&& id.len() == 16
			&& id.bytes().all(|b| b.is_ascii_hexdigit())
		{
			return id;
		}
		// Randomly seeded.
		let id = format!("{:016x}", RandomState::new().build_hasher().finish());
		let _ = write_file(&self.id, id.as_bytes(), 0o600);
		id
	}
}

/// What transfers the database, to be able to test without a server.
trait Server {
	/// Downloads the server copy to `files.download`.
	fn download(&self, files: &Files, until: Option<Instant>) -> Result<(), Failure>;
	/// Replaces the server copy with `files.uploading`, after downloading what gets
	/// replaced to `files.replaced`. Without `must_exist`, there must be no server copy.
	fn replace(&self, files: &Files, must_exist: bool, mode: Option<u32>, until: Option<Instant>) -> Result<(), Failure>;
}

impl Server for Target {
	fn download(&self, files: &Files, until: Option<Instant>) -> Result<(), Failure> {
		create_dir(files.dir()).map_err(|err| Failure::Failed(format!("couldn't create {}: {err}", files.dir().display())))?;
		Self::download(self, files.dir(), name(&files.download), until)
	}

	fn replace(&self, files: &Files, must_exist: bool, mode: Option<u32>, until: Option<Instant>) -> Result<(), Failure> {
		let upload = Upload { name: name(&files.uploading), replaced: name(&files.replaced), must_exist, mode, id: &files.device_id() };
		Self::replace(self, files.dir(), &upload, until)
	}
}

/// How the local copy relates to the server.
#[derive(Default)]
pub struct SyncState {
	/// Shown on the login screen when the server copy couldn't be used.
	pub note: Option<String>,
	/// The local copy can't be used, so there's nothing to open or create.
	pub blocked: bool,
	/// Try to open it again, after the next redraw.
	pub retry: bool,
	/// The local copy has changes that aren't on the server yet.
	pub unsynced: bool,
	/// An upload is due, after the next redraw.
	pub pending: bool,
	/// Keeps other kptui from using the local copy.
	lock: Option<File>,
	/// The local copy as of the last check, to notice writes to it.
	seen: Option<(Option<SystemTime>, u64)>,
}

/// Updates the local copy from the server, when the database gets selected
/// (on start, or in the Config screen). This needs no master password.
pub fn open(app: &mut App) {
	// This also releases the lock on the previous database.
	app.remote = SyncState::default();
	let Some(remote) = app.config.remote.as_ref() else {
		return;
	};
	let files = remote.files();
	let target = remote.target();
	let problem = match files.lock() {
		Ok(Some(lock)) => {
			app.remote.lock = Some(lock);
			open_with(app, &files, target.as_ref().map_err(Failure::to_string));
			return;
		}
		Ok(None) => "it's open in another kptui".to_string(),
		Err(err) => format!("{err:#}"),
	};
	app.remote.note = Some(format!("Can't open it: {problem}"));
	app.remote.blocked = true;
}

fn open_with(app: &mut App, files: &Files, server: Result<&impl Server, String>) {
	let has_cache = files.cache.is_file();
	let unsynced = has_cache && files.unsynced();
	let problem = match server.and_then(|server| match server.download(files, None) {
		Err(Failure::Missing) if !has_cache => Ok(false),
		result => result.map(|()| true).map_err(|failure| failure.to_string()),
	}) {
		Ok(true) if crate::db::kdbx_version(&files.download).is_none() => Some("the server file isn't a KeePass database".to_string()),
		Ok(true) if !unsynced => install(files).err().map(|err| format!("{err:#}")),
		Ok(_) => None,
		Err(problem) => Some(problem),
	};
	remove(&files.download);

	let state = &mut app.remote;
	state.unsynced = unsynced;
	match problem {
		Some(problem) if has_cache => state.note = Some(format!("Using the local copy: {problem}")),
		Some(problem) => {
			state.note = Some(format!("Can't open it: {problem}"));
			state.blocked = true;
		}
		None if unsynced => state.note = Some("Local changes get uploaded after unlocking".to_string()),
		None => {}
	}
}

/// Tries again to open a database that can't be used.
pub fn retry(app: &mut App) {
	open(app);
	app.db_version = app.config.default_database.as_deref().and_then(crate::db::kdbx_version);
}

/// Makes the download the local copy and the base. If this gets interrupted, the
/// local copy isn't the base, but it is what's on the server: nothing to upload.
fn install(files: &Files) -> anyhow::Result<()> {
	write_file(&files.cache, &fs::read(&files.download).context("couldn't read the server copy")?, 0o600)?;
	keep(&files.download, &files.base)
}

/// Schedules an upload when the local copy changed since the last check.
/// Called after anything that may have written to it.
pub fn note_writes(app: &mut App) {
	let Some(remote) = app.config.remote.as_ref() else {
		return;
	};
	if app.kdbx.is_none() || app.remote.pending {
		return;
	}

	let files = remote.files();
	let stamp = stamp(&files.cache);
	if stamp == app.remote.seen {
		return;
	}
	app.remote.seen = stamp;
	app.remote.unsynced = files.unsynced();
	if app.remote.unsynced {
		app.remote.pending = true;
		app.status = Some(match app.status.take() {
			Some(status) if !status.starts_with("Saving") => format!("{status}{UPLOADING}"),
			_ => "Uploading...".to_string(),
		});
	}
}

/// Does the upload scheduled by `note_writes`.
pub fn process_pending(app: &mut App) {
	if !app.remote.pending {
		return;
	}
	app.remote.pending = false;

	let Some(remote) = app.config.remote.as_ref() else {
		return;
	};
	let files = remote.files();
	let target = remote.target();
	let mut log = Log::default();
	let result = match target {
		Ok(target) => sync(app, &target, &files, &mut log),
		Err(failure) => Err(failure.into()),
	};
	remove(&files.download);
	app.remote.seen = stamp(&files.cache);
	app.remote.unsynced = files.unsynced();

	let message = log.describe(result);
	app.status = Some(match app.status.take().and_then(|status| status.strip_suffix(UPLOADING).map(str::to_string)) {
		Some(done) => format!("{done}; {message}"),
		None => capitalize(&message),
	});
}

#[derive(Debug, PartialEq, Eq)]
enum Done {
	Uploaded,
	UpToDate,
}

/// What a sync did besides uploading.
#[derive(Default)]
struct Log {
	merged: Option<merge::Report>,
	/// Versions from other devices that couldn't be merged.
	set_aside: Vec<PathBuf>,
}

impl Log {
	fn describe(&self, result: anyhow::Result<Done>) -> String {
		let mut message = match (result, &self.merged) {
			(Ok(Done::Uploaded), None) => "uploaded".to_string(),
			(Ok(Done::UpToDate), None) => "already up to date on the server".to_string(),
			(Ok(_), Some(report)) => format!("merged with changes from the server and uploaded{}", merge_notes(report)),
			(Err(err), None) => format!("not uploaded: {err:#}"),
			(Err(err), Some(report)) => format!("merged with changes from the server, not uploaded: {err:#}{}", merge_notes(report)),
		};
		for path in &self.set_aside {
			message.push_str(&format!("; couldn't merge a version from another device, kept in {}", path.display()));
		}
		message
	}
}

/// Uploads the local copy, merging it with the server copy first if that changed.
fn sync(app: &mut App, server: &impl Server, files: &Files, log: &mut Log) -> anyhow::Result<Done> {
	let until = auto_exit(app);
	recover(app, server, files, until, log)?;

	for _ in 0..ROUNDS {
		let exists = match server.download(files, until) {
			Ok(()) => true,
			Err(Failure::Missing) => false,
			Err(failure) => return Err(failure.into()),
		};
		if exists {
			ensure!(crate::db::kdbx_version(&files.download).is_some(), "the server file isn't a KeePass database, so it wasn't replaced");
			if same_contents(&files.download, &files.cache) {
				keep(&files.download, &files.base)?;
				return Ok(Done::UpToDate);
			}
			if !same_contents(&files.download, &files.base) {
				let other = open_version(app, &files.download)?.context("the server copy changed, and it's another database or has another master key")?;
				merge_with(app, files, &other, log)?;
			}
			// The local copy includes the server copy now, so that's what it started
			// from. This also gives the base the current permissions on the server.
			keep(&files.download, &files.base)?;
		}

		let mode = if exists { file_mode(&files.base) } else { None };
		write_file(&files.uploading, &fs::read(&files.cache).context("couldn't read the local copy")?, mode.unwrap_or(0o600))?;
		match server.replace(files, exists, mode, until) {
			Ok(()) => {}
			// Deleted meanwhile.
			Err(Failure::Missing) => continue,
			Err(failure @ Failure::Failed(_)) => {
				// Nothing was replaced.
				remove(&files.replaced);
				remove(&files.uploading);
				if !exists && server.download(files, until).is_ok() {
					// Created meanwhile.
					continue;
				}
				return Err(failure.into());
			}
			// It's unknown whether it was replaced, see `recover`.
			Err(failure) => return Err(failure.into()),
		}

		let expected = !exists || same_contents(&files.replaced, &files.base);
		if !expected {
			// Another device saved between the download and the upload.
			absorb(app, files, log)?;
		}
		// In this order: compared with the new base, what it replaced looks like
		// the changes just uploaded were undone.
		remove(&files.replaced);
		keep(&files.uploading, &files.base)?;
		if expected {
			return Ok(Done::Uploaded);
		}
	}
	bail!("the server copy keeps changing")
}

/// Finishes an upload that got interrupted, because kptui got killed, the
/// connection was lost, or it was time for auto-exit.
fn recover(app: &mut App, server: &impl Server, files: &Files, until: Option<Instant>, log: &mut Log) -> anyhow::Result<()> {
	if !files.uploading.is_file() && !files.replaced.is_file() {
		return Ok(());
	}
	match server.download(files, until) {
		Ok(()) | Err(Failure::Missing) => {}
		Err(failure) => return Err(failure.into()),
	}

	let on_server = |path: &Path| same_contents(&files.download, path);
	// What's still on the server wasn't replaced, and can be incomplete here.
	let still_there = on_server(&files.base) || starts_with(&files.download, &files.replaced);
	if files.replaced.is_file() && !still_there && !same_contents(&files.replaced, &files.base) && !same_contents(&files.replaced, &files.cache) {
		absorb(app, files, log)?;
	}
	remove(&files.replaced);
	if on_server(&files.uploading) {
		return keep(&files.uploading, &files.base);
	}
	remove(&files.uploading);
	Ok(())
}

/// Merges what an upload replaced. If that's not possible, it's kept.
fn absorb(app: &mut App, files: &Files, log: &mut Log) -> anyhow::Result<()> {
	if let Some(other) = open_version(app, &files.replaced)? {
		return merge_with(app, files, &other, log);
	}
	let mut number = 1;
	let path = loop {
		let path = with_suffix(&files.cache, &format!(".unmerged-{number}"));
		if !path.exists() {
			break path;
		}
		number += 1;
	};
	keep(&files.replaced, &path)?;
	log.set_aside.push(path);
	Ok(())
}

/// Opens another version of the open database, or `None` if it isn't one.
fn open_version(app: &App, path: &Path) -> anyhow::Result<Option<Database>> {
	let (Some(db), Some(key)) = (app.kdbx.as_ref(), app.db_key.as_ref()) else {
		bail!("the database is locked");
	};
	// Decrypting takes a while.
	check_auto_exit(app)?;
	Ok(crate::db::open_database(path, key).ok().filter(|other| other.root().id() == db.root().id()))
}

/// Merges another version into the open database, and saves the local copy.
fn merge_with(app: &mut App, files: &Files, other: &Database, log: &mut Log) -> anyhow::Result<()> {
	// Without a usable base, nothing counts as deleted.
	let base = if files.base.is_file() { open_version(app, &files.base)? } else { None };
	check_auto_exit(app)?;
	let (Some(local), Some(key)) = (app.kdbx.as_ref(), app.db_key.as_ref()) else {
		bail!("the database is locked");
	};
	let (mut merged, report) = merge::merge(base.as_ref(), local, other);
	crate::db::save_database(&files.cache, key, &mut merged, &mut [])?;
	reload(app, merged);

	let total = log.merged.get_or_insert_default();
	total.conflicts += report.conflicts;
	total.kept += report.kept;
	Ok(())
}

/// When it's time for auto-exit, which transfers must not delay.
fn auto_exit(app: &App) -> Option<Instant> {
	let after = app.config.auto_exit;
	if after.is_zero() { None } else { app.last_activity.checked_add(after) }
}

fn check_auto_exit(app: &App) -> Result<(), Failure> {
	if auto_exit(app).is_some_and(|until| Instant::now() >= until) { Err(Failure::Stopped) } else { Ok(()) }
}

/// What the merge resolved, to add to the status.
fn merge_notes(report: &merge::Report) -> String {
	let mut notes = Vec::new();
	match report.conflicts {
		0 => {}
		1 => notes.push("1 conflict, kept as a copy".to_string()),
		n => notes.push(format!("{n} conflicts, kept as copies")),
	}
	match report.kept {
		0 => {}
		1 => notes.push("1 deleted entry kept, it was changed".to_string()),
		n => notes.push(format!("{n} deleted entries kept, they were changed")),
	}
	if notes.is_empty() { String::new() } else { format!(" ({})", notes.join("; ")) }
}

/// Shows a merged database, keeping the entry being viewed open.
fn reload(app: &mut App, db: Database) {
	let viewed = app.edit_entry.as_ref().and_then(|entry| entry.id).filter(|_| app.screen == Screen::Edit);
	app.entries = crate::db::entries_of(&db);
	app.kdbx = Some(db);
	app.refresh_filter();
	if app.screen != Screen::Edit {
		return;
	}
	match viewed
		.and_then(|id| app.entries.iter().position(|entry| entry.id == Some(id)))
		.and_then(|index| app.row_of_entry(index))
	{
		Some(row) => {
			app.index_state.select(Some(row));
			crate::input::command::preview_entry(app);
		}
		None => crate::input::edit::reset_edit_state(app),
	}
}

fn name(path: &Path) -> &str {
	path.file_name().and_then(OsStr::to_str).unwrap_or_default()
}

fn remove(path: &Path) {
	let _ = fs::remove_file(path);
}

fn keep(from: &Path, to: &Path) -> anyhow::Result<()> {
	fs::rename(from, to).with_context(|| format!("couldn't store {}", to.display()))
}

fn same_contents(a: &Path, b: &Path) -> bool {
	matches!((fs::read(a), fs::read(b)), (Ok(a), Ok(b)) if a == b)
}

fn starts_with(path: &Path, start: &Path) -> bool {
	matches!((fs::read(path), fs::read(start)), (Ok(path), Ok(start)) if path.starts_with(&start))
}

fn stamp(path: &Path) -> Option<(Option<SystemTime>, u64)> {
	fs::metadata(path).ok().map(|metadata| (metadata.modified().ok(), metadata.len()))
}

fn file_mode(path: &Path) -> Option<u32> {
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::metadata(path).ok().map(|metadata| metadata.permissions().mode() & 0o777)
	}
	#[cfg(not(unix))]
	{
		let _ = path;
		None
	}
}

fn create_dir(dir: &Path) -> std::io::Result<()> {
	let mut builder = fs::DirBuilder::new();
	builder.recursive(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::DirBuilderExt;
		builder.mode(0o700);
	}
	builder.create(dir)
}

/// Replaces a file in one go, with the given permissions.
fn write_file(path: &Path, bytes: &[u8], mode: u32) -> anyhow::Result<()> {
	let tmp = with_suffix(path, ".tmp");
	let _ = fs::remove_file(&tmp);
	let result = (|| -> std::io::Result<()> {
		let mut options = OpenOptions::new();
		options.write(true).create_new(true);
		#[cfg(unix)]
		{
			use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
			options.mode(0o600);
			let file = options.open(&tmp)?;
			// Unlike the mode at creation, this isn't reduced by the umask.
			file.set_permissions(fs::Permissions::from_mode(mode))?;
			write_and_sync(file, bytes)?;
		}
		#[cfg(not(unix))]
		{
			let _ = mode;
			write_and_sync(options.open(&tmp)?, bytes)?;
		}
		fs::rename(&tmp, path)
	})();
	if result.is_err() {
		let _ = fs::remove_file(&tmp);
	}
	result.with_context(|| format!("couldn't write {}", path.display()))
}

fn write_and_sync(mut file: fs::File, bytes: &[u8]) -> std::io::Result<()> {
	file.write_all(bytes)?;
	file.sync_all()
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
	let mut name = OsString::from(path.as_os_str());
	name.push(suffix);
	PathBuf::from(name)
}

fn capitalize(text: &str) -> String {
	let mut chars = text.chars();
	chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// FNV-1a: a stable hash, unlike std's, to tell local copies apart.
fn fnv1a(bytes: &[u8]) -> u64 {
	bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::db::{create_database, entries_of, open_database, save_database};
	use keepass::DatabaseKey;
	use keepass::db::fields;
	use std::cell::{Cell, RefCell};
	use std::time::Duration;
	use zeroize::Zeroizing;

	#[test]
	fn local_copies_are_named_after_host_and_file() {
		let path = Remote::new("sftp://me@nas:2222/srv/Sync Configs/My Vault.kdbx").cache_path();
		let name = path.file_name().and_then(|name| name.to_str()).expect("name should be UTF-8");
		assert_eq!(path.parent(), Some(expand_tilde(DIR).as_path()));
		assert!(name.starts_with("nas-My_Vault-"), "{name}");
		assert!(name.ends_with(".kdbx"), "{name}");
		assert_ne!(path, Remote::new("sftp://me@nas:2222/srv/Other/My Vault.kdbx").cache_path());
		assert_eq!(path, Remote::new("  sftp://me@nas:2222/srv/Sync Configs/My Vault.kdbx ").cache_path());
	}

	#[test]
	fn hashes_are_stable() {
		assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
		assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
	}

	#[test]
	fn describes_what_the_merge_resolved() {
		assert_eq!(merge_notes(&merge::Report::default()), "");
		assert_eq!(merge_notes(&merge::Report { conflicts: 1, kept: 0 }), " (1 conflict, kept as a copy)");
		assert_eq!(
			merge_notes(&merge::Report { conflicts: 2, kept: 1 }),
			" (2 conflicts, kept as copies; 1 deleted entry kept, it was changed)"
		);
	}

	#[test]
	fn capitalizes_messages() {
		assert_eq!(capitalize("uploaded"), "Uploaded");
		assert_eq!(capitalize(""), "");
	}

	const URL: &str = "sftp://nas.invalid/vault.kdbx";
	const PASSWORD: &str = "correct horse";

	/// What another device does, given the server copy.
	type Action = Box<dyn FnOnce(&FakeServer)>;

	#[derive(Default)]
	struct FakeServer {
		file: RefCell<Option<Vec<u8>>>,
		/// Run between the checks and the replacement of the next uploads.
		during_uploads: RefCell<Vec<Action>>,
		uploads: Cell<usize>,
		/// When to lose the connection during the next upload.
		lose_connection: Cell<Lose>,
		slow_download: Cell<Duration>,
	}

	#[derive(Clone, Copy, Default, PartialEq, Eq)]
	enum Lose {
		#[default]
		Never,
		/// While downloading what's about to be replaced.
		Download,
		/// Before the rename.
		Before,
		/// After the rename.
		After,
	}

	impl Server for FakeServer {
		fn download(&self, files: &Files, until: Option<Instant>) -> Result<(), Failure> {
			if until.is_some_and(|until| Instant::now() >= until) {
				return Err(Failure::Stopped);
			}
			std::thread::sleep(self.slow_download.get());
			remove(&files.download);
			let file = self.file.borrow().clone().ok_or(Failure::Missing)?;
			fs::write(&files.download, file).map_err(|err| Failure::Failed(err.to_string()))
		}

		fn replace(&self, files: &Files, must_exist: bool, _mode: Option<u32>, _until: Option<Instant>) -> Result<(), Failure> {
			let action = if self.during_uploads.borrow().is_empty() { None } else { Some(self.during_uploads.borrow_mut().remove(0)) };
			if let Some(action) = action {
				action(self);
			}
			let lose = self.lose_connection.take();
			let lost = || Err(Failure::Unreachable("connection lost".to_string()));
			remove(&files.replaced);
			match self.file.borrow().as_ref() {
				Some(_) if !must_exist => return Err(Failure::Failed("Failure".to_string())),
				Some(file) => {
					let length = if lose == Lose::Download { file.len() / 2 } else { file.len() };
					fs::write(&files.replaced, &file[..length]).expect("replaced file should be written");
				}
				None if must_exist => return Err(Failure::Missing),
				None => {}
			}
			if matches!(lose, Lose::Download | Lose::Before) {
				return lost();
			}
			*self.file.borrow_mut() = Some(fs::read(&files.uploading).expect("upload should be readable"));
			self.uploads.set(self.uploads.get() + 1);
			if lose == Lose::After { lost() } else { Ok(()) }
		}
	}

	impl FakeServer {
		/// Another device opens the server copy, changes it, and saves it.
		fn edit(&self, change: impl FnOnce(&mut Database)) {
			let path = expand_tilde("~/other-device.kdbx");
			fs::write(&path, self.file.borrow().as_ref().expect("the file should exist")).expect("file should be written");
			let mut db = open_database(&path, &key()).expect("server copy should open");
			change(&mut db);
			save_database(&path, &key(), &mut db, &mut []).expect("server copy should be saved");
			*self.file.borrow_mut() = Some(fs::read(&path).expect("file should be readable"));
		}

		fn titles(&self) -> Vec<String> {
			let path = expand_tilde("~/server-copy.kdbx");
			fs::write(&path, self.file.borrow().as_ref().expect("the file should exist")).expect("file should be written");
			titles(&open_database(&path, &key()).expect("server copy should open"))
		}
	}

	fn key() -> DatabaseKey {
		DatabaseKey::new().with_password(PASSWORD)
	}

	fn add(db: &mut Database, title: &str) {
		db.root_mut().add_entry().set_unprotected(fields::TITLE, title);
	}

	fn titles(db: &Database) -> Vec<String> {
		let mut titles: Vec<String> = db.iter_all_entries().map(|entry| entry.get_title().unwrap_or_default().to_string()).collect();
		titles.sort();
		titles
	}

	/// An app with the database open, holding "Mail", in sync with the server.
	fn setup() -> (App, FakeServer, Files) {
		let mut app = crate::app::start(false, Zeroizing::new(String::new()));
		app.config.set_database(URL);
		app.config.keyfile = None;
		let files = app.config.remote.as_ref().expect("database should be remote").files();
		let _ = fs::remove_dir_all(files.dir());
		app.remote.lock = files.lock().expect("lock should work");
		assert!(app.remote.lock.is_some());
		let (mut db, _, _) =
			create_database(&files.cache, 1, &Zeroizing::new(PASSWORD.to_string()), None, crate::config::MIB).expect("database should be created");
		add(&mut db, "Mail");
		save_database(&files.cache, &key(), &mut db, &mut []).expect("database should be saved");
		fs::copy(&files.cache, &files.base).expect("base should be written");
		let server = FakeServer::default();
		*server.file.borrow_mut() = Some(fs::read(&files.cache).expect("local copy should be readable"));
		app.entries = entries_of(&db);
		app.kdbx = Some(db);
		app.db_key = Some(key());
		app.config.auto_exit = Duration::ZERO;
		(app, server, files)
	}

	/// Adds an entry in the app, and saves it to the local copy.
	fn save_entry(app: &mut App, files: &Files, title: &str) {
		let db = app.kdbx.as_mut().expect("database should be open");
		add(db, title);
		save_database(&files.cache, &key(), db, &mut []).expect("local copy should be saved");
		app.entries = entries_of(db);
	}

	fn run(app: &mut App, server: &FakeServer, files: &Files) -> (anyhow::Result<Done>, Log) {
		let mut log = Log::default();
		(sync(app, server, files, &mut log), log)
	}

	#[test]
	fn uploads_a_change() {
		let (mut app, server, files) = setup();
		save_entry(&mut app, &files, "Bank");
		assert!(files.unsynced());
		let (result, log) = run(&mut app, &server, &files);
		assert_eq!(result.expect("sync should work"), Done::Uploaded);
		assert_eq!(log.describe(Ok(Done::Uploaded)), "uploaded");
		assert_eq!(server.titles(), ["Bank", "Mail"]);
		assert!(!files.unsynced());

		let (result, _) = run(&mut app, &server, &files);
		assert_eq!(result.expect("sync should work"), Done::UpToDate);
		assert_eq!(server.uploads.get(), 1);
	}

	#[test]
	fn merges_a_change_from_another_device() {
		let (mut app, server, files) = setup();
		server.edit(|db| add(db, "Shop"));
		save_entry(&mut app, &files, "Bank");
		let (result, log) = run(&mut app, &server, &files);
		assert_eq!(log.describe(result), "merged with changes from the server and uploaded");
		assert_eq!(server.titles(), ["Bank", "Mail", "Shop"]);
		assert_eq!(app.entries.len(), 3);
		assert!(!files.unsynced());
	}

	#[test]
	fn keeps_what_another_device_saves_during_the_upload() {
		let (mut app, server, files) = setup();
		server.during_uploads.borrow_mut().push(Box::new(|server| server.edit(|db| add(db, "Shop"))));
		server.during_uploads.borrow_mut().push(Box::new(|server| server.edit(|db| add(db, "Forum"))));
		save_entry(&mut app, &files, "Bank");
		let (result, log) = run(&mut app, &server, &files);
		assert_eq!(log.describe(result), "merged with changes from the server and uploaded");
		assert_eq!(server.titles(), ["Bank", "Forum", "Mail", "Shop"]);
		assert_eq!(server.uploads.get(), 3);
		assert!(!files.unsynced());
	}

	#[test]
	fn keeps_a_file_that_another_device_creates_during_the_upload() {
		let (mut app, server, files) = setup();
		let first = server.file.take();
		server.during_uploads.borrow_mut().push(Box::new(move |server| {
			*server.file.borrow_mut() = first;
			server.edit(|db| add(db, "Shop"));
		}));
		save_entry(&mut app, &files, "Bank");
		run(&mut app, &server, &files).0.expect("sync should work");
		assert_eq!(server.titles(), ["Bank", "Mail", "Shop"]);
	}

	#[test]
	fn gives_up_when_the_server_copy_keeps_changing_but_loses_nothing() {
		let (mut app, server, files) = setup();
		for i in 0..ROUNDS {
			server.during_uploads.borrow_mut().push(Box::new(move |server| server.edit(|db| add(db, &format!("Shop {i}")))));
		}
		save_entry(&mut app, &files, "Bank");
		let (result, log) = run(&mut app, &server, &files);
		assert!(log.describe(result).contains("not uploaded: the server copy keeps changing"));
		assert!(files.unsynced());

		run(&mut app, &server, &files).0.expect("sync should work");
		assert_eq!(server.titles().len(), 2 + ROUNDS);
	}

	#[test]
	fn an_interrupted_upload_gets_finished() {
		let shop = || -> Action { Box::new(|server| server.edit(|db| add(db, "Shop"))) };
		for lose in [Lose::Download, Lose::Before, Lose::After] {
			for saved_elsewhere in [false, true] {
				let (mut app, server, files) = setup();
				if saved_elsewhere {
					server.during_uploads.borrow_mut().push(shop());
				}
				server.lose_connection.set(lose);
				save_entry(&mut app, &files, "Bank");
				run(&mut app, &server, &files).0.expect_err("the connection should get lost");
				assert!(files.unsynced());
				if lose == Lose::After {
					assert_eq!(server.titles(), ["Bank", "Mail"], "replaced, and not merged yet");
				}

				let (result, log) = run(&mut app, &server, &files);
				result.expect("sync should work");
				assert_eq!(log.merged.is_some(), saved_elsewhere);
				assert!(log.set_aside.is_empty());
				if saved_elsewhere {
					assert_eq!(server.titles(), ["Bank", "Mail", "Shop"]);
				} else {
					assert_eq!(server.titles(), ["Bank", "Mail"]);
				}
				assert!(!files.unsynced() && !files.uploading.exists() && !files.replaced.exists());
			}
		}
	}

	#[test]
	fn an_interrupted_upload_gets_finished_after_a_save_elsewhere() {
		// Another device saves once more, after the connection got lost.
		for lose in [Lose::Before, Lose::After] {
			let (mut app, server, files) = setup();
			server.during_uploads.borrow_mut().push(Box::new(|server| server.edit(|db| add(db, "Shop"))));
			server.lose_connection.set(lose);
			save_entry(&mut app, &files, "Bank");
			run(&mut app, &server, &files).0.expect_err("the connection should get lost");
			server.edit(|db| add(db, "Forum"));
			save_entry(&mut app, &files, "Notes");

			let (result, log) = run(&mut app, &server, &files);
			result.expect("sync should work");
			assert_eq!(log.merged, Some(merge::Report::default()));
			assert_eq!(server.titles(), ["Bank", "Forum", "Mail", "Notes", "Shop"]);
		}
	}

	#[test]
	fn no_change_gets_undone_when_kptui_dies_while_finishing_an_upload() {
		// What was replaced has the old title, which must not undo the new one.
		let rename = |db: &mut Database| {
			let id = db.iter_all_entries().next().expect("there should be an entry").id();
			db.entry_mut(id).expect("entry should exist").set_unprotected(fields::TITLE, "Renamed");
		};
		for merged in [false, true] {
			let (mut app, server, files) = setup();
			server.during_uploads.borrow_mut().push(Box::new(|server| server.edit(|db| add(db, "Shop"))));
			server.lose_connection.set(Lose::After);
			let db = app.kdbx.as_mut().expect("database should be open");
			rename(db);
			save_database(&files.cache, &key(), db, &mut []).expect("local copy should be saved");
			run(&mut app, &server, &files).0.expect_err("the connection should get lost");
			if merged {
				// It dies between merging and removing what was replaced.
				absorb(&mut app, &files, &mut Log::default()).expect("it should be merged");
			}
			run(&mut app, &server, &files).0.expect("sync should work");
			assert_eq!(server.titles(), ["Renamed", "Shop"]);
		}
	}

	#[test]
	fn keeps_a_replaced_version_that_cannot_be_merged() {
		let (mut app, server, files) = setup();
		server.during_uploads.borrow_mut().push(Box::new(|server| *server.file.borrow_mut() = Some(b"not a database".to_vec())));
		save_entry(&mut app, &files, "Bank");
		let (result, log) = run(&mut app, &server, &files);
		result.expect("sync should work");
		let [path] = log.set_aside.as_slice() else { panic!("it should be kept") };
		assert_eq!(fs::read(path).expect("it should be readable"), b"not a database");
		assert_eq!(server.titles(), ["Bank", "Mail"]);
	}

	#[test]
	fn never_merges_or_replaces_another_database() {
		let (mut app, server, files) = setup();
		let path = expand_tilde("~/another.kdbx");
		create_database(&path, 1, &Zeroizing::new(PASSWORD.to_string()), None, crate::config::MIB).expect("database should be created");
		let another = fs::read(&path).expect("file should be readable");
		*server.file.borrow_mut() = Some(another.clone());
		save_entry(&mut app, &files, "Bank");
		run(&mut app, &server, &files).0.expect_err("it's another database");
		assert_eq!(server.file.borrow().as_ref(), Some(&another));
		assert_eq!(titles(app.kdbx.as_ref().expect("database should be open")), ["Bank", "Mail"]);
	}

	#[test]
	fn an_interrupted_update_of_the_local_copy_uploads_nothing() {
		let (mut app, server, files) = setup();
		server.edit(|db| add(db, "Shop"));
		// Only the first half of `install`.
		fs::write(&files.cache, server.file.borrow().as_ref().expect("the file should exist")).expect("local copy should be written");
		app.kdbx = Some(open_database(&files.cache, &key()).expect("local copy should open"));
		assert!(files.unsynced());
		assert_eq!(run(&mut app, &server, &files).0.expect("sync should work"), Done::UpToDate);
		assert_eq!((server.uploads.get(), server.titles()), (0, vec!["Mail".to_string(), "Shop".to_string()]));
	}

	#[test]
	fn opening_updates_the_local_copy_unless_it_has_changes() {
		let (mut app, server, files) = setup();
		server.edit(|db| add(db, "Shop"));
		open_with(&mut app, &files, Ok(&server));
		assert_eq!(app.remote.note, None);
		assert_eq!(titles(&open_database(&files.cache, &key()).expect("local copy should open")), ["Mail", "Shop"]);
		assert!(!files.unsynced());

		app.kdbx = Some(open_database(&files.cache, &key()).expect("local copy should open"));
		save_entry(&mut app, &files, "Bank");
		server.edit(|db| add(db, "Forum"));
		open_with(&mut app, &files, Ok(&server));
		assert_eq!(app.remote.note.as_deref(), Some("Local changes get uploaded after unlocking"));
		assert_eq!(titles(&open_database(&files.cache, &key()).expect("local copy should open")), ["Bank", "Mail", "Shop"]);

		open_with(&mut app, &files, Err::<&FakeServer, _>("no network".to_string()));
		assert_eq!(app.remote.note.as_deref(), Some("Using the local copy: no network"));
		assert!(!app.remote.blocked);
		assert_eq!(server.uploads.get(), 0, "opening never uploads");
	}

	#[test]
	fn only_one_kptui_uses_the_local_copy() {
		let (mut app, _, files) = setup();
		assert!(files.lock().expect("locking should work").is_none(), "it's locked already");
		let locked = app.remote.lock.take();
		app.kdbx = None;
		open(&mut app);
		assert!(app.remote.blocked);
		assert_eq!(app.remote.note.as_deref(), Some("Can't open it: it's open in another kptui"));

		// It can't be unlocked, either.
		let unlock = |app: &mut App| {
			app.password.push_str(PASSWORD);
			crate::input::login::attempt_unlock(app);
			assert!(app.password.is_empty());
			app.kdbx.is_some()
		};
		assert!(!unlock(&mut app));
		app.remote.blocked = false;
		assert!(unlock(&mut app), "{:?}", app.login_error);

		drop(locked);
		assert!(files.lock().expect("locking should work").is_some());
	}

	#[test]
	fn auto_exit_breaks_off_a_sync() {
		let (mut app, server, files) = setup();
		save_entry(&mut app, &files, "Bank");
		app.config.auto_exit = Duration::from_millis(1);
		std::thread::sleep(Duration::from_millis(5));
		let (result, log) = run(&mut app, &server, &files);
		assert_eq!(log.describe(result), "not uploaded: stopped by auto-exit");

		// It also stops before decrypting and merging, which take a while.
		server.edit(|db| add(db, "Shop"));
		server.slow_download.set(Duration::from_secs(2));
		app.config.auto_exit = Duration::from_secs(1);
		app.last_activity = Instant::now();
		let (result, log) = run(&mut app, &server, &files);
		assert_eq!(log.describe(result), "not uploaded: stopped by auto-exit");
		assert_eq!(server.uploads.get(), 0);
		assert!(files.unsynced());

		app.config.auto_exit = Duration::from_secs(u64::MAX);
		assert_eq!(auto_exit(&app), None, "a far too long time is never");
	}

	#[test]
	fn every_device_has_its_own_id() {
		let (_app, _, files) = setup();
		let id = files.device_id();
		assert_eq!(id.len(), 16);
		assert_eq!(files.device_id(), id);
		remove(&files.id);
		assert_ne!(files.device_id(), id);
	}
}
