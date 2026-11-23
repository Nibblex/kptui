[![version](https://img.shields.io/crates/v/kptui.svg)](https://crates.io/crates/kptui)
[![build](https://github.com/pepa65/kptui/actions/workflows/rust.yml/badge.svg)](https://github.com/pepa65/kptui/actions/workflows/rust.yml)
[![dependencies](https://deps.rs/repo/github/pepa65/kptui/status.svg)](https://deps.rs/repo/github/pepa65/kptui)
[![docs](https://img.shields.io/badge/docs-kptui-blue.svg)](https://docs.rs/crate/kptui/latest)
[![license](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://github.com/pepa65/kptui/blob/main/LICENSE.md)
[![downloads](https://img.shields.io/crates/d/kptui.svg)](https://crates.io/crates/kptui)

# kptui 0.39.0-nibblex.1
## About this fork
This is a fork of [pepa65/kptui](https://github.com/pepa65/kptui), which deliberately stays small.
It adds what a KeePassXC user on a text console or over SSH may miss:
* **Copy to tmux** (`Ctrl-y`): copies the selected field to a tmux paste buffer, cleared after a delay (opt-in).
  Proposed upstream as [#1](https://github.com/pepa65/kptui/pull/1).
* **Hidden passwords** (`Ctrl-r`): passwords show as dots until shown.
  Proposed upstream as [#4](https://github.com/pepa65/kptui/pull/4).
* **Remote vaults over SFTP**: `database = "sftp://host/path/vault.kdbx"`, used through a local copy,
  with changes from other devices merged in when saving.
* **Groups**: the Index browses the groups (folders) of the vault.
* **Additional attributes**: the custom fields of KeePassXC's Advanced section, shown and editable.

To install it: `cargo install --git https://github.com/Nibblex/kptui --branch nibblex --locked kptui`,
or get a binary from the [releases](https://github.com/Nibblex/kptui/releases).
`kptui version` shows the fork's version, like `0.39.0-nibblex.1`. Everything below describes this fork,
but the download links in "Installing" are upstream's.

**Highly secure TUI password manager for [KeePass](https://keepass.info/) KDBX vaults (includes Linux GUI)**
* Compatible with the PassKeeper/KeePass KDBX `.kdb` and `.kdbx` file format.
* By default, it preserves the database format of `KDBX4.0` or `KDBX4.1` files, which can be used by:
  KeePassXC, KeePassDX/KeePass2Android, KeeWeb and KeePass2, so fits in with the wider KeePass ecosystem.
  - On the Configuration screen, the version can be switched between `KDBX4.0` and `KDBX4.1`.
  - If a file has a different `KDBX` format, it will save it to `KDBX4.1` format.
* Very secure in its default configuration, impossible to brute-force (so unlocking takes a few seconds!).
  The password is hardened with Argon2 [hardcoded]:
  - KDF-Memory: `256 MiB` [configurable]
  - KDF-Iterations: `90` (!) [configurable]
  - KDF-Parallel: `4` [hardcoded]
* Stores: `Name` of the entry, `Username`, `Password`, `TOTP`, `URL` and `Notes` (and the `Last modified` time),
  and keeps the groups and additional attributes.
* The Terminal User Interface is responsive and fully keyboard-driven, and a stand-alone static binary.
* The Graphical User Interface version runs on X11 and Wayland Linux and is even more secure in wiping all secrets.
  This binary (`kpgui`) is dynamically linked to C and graphical system libraries.

## Features
* **KDBX4.1 vaults** (or `KDBX4.0`): Reads and writes standard `.kdbx` files, so you can open the same vault
  in KeePassXC, KeePassDX/KeePass2Android, KeeWeb, KeePass2 or any other compatible client.
* **Optional keyfile support**: Can unlocks vaults protected by a master password plus a keyfile,
  but password-only vaults are also supported.
* **Configurable KDF iterations**: If more or less paranoid (the default `90` is a high setting).
* **Configurable KDF memory**: The default for new databases is `256 MiB` (requiring RAM usage for most CPUs).
* **Filtered search**: Start typing on the index screen to filter entries on Name & User,
  or on Name/User/Notes (toggled by `Ctrl-f`).
* **Groups**: The Index browses the groups (folders) of the vault, like KeePass on the desktop:
  the subgroups of the current group come first, with a `/` after their name, then its entries.
  - `Enter` (or `Right`) opens a group, and `Esc` (or `Left`) goes back up. The title shows where you are, like `/Internet/Email`.
  - Searching looks in all groups, and then shows the group of each entry instead of its date.
  - New entries (`Ctrl-n`) go into the current group, and the title of an entry shows its group.
  - `Esc` first clears the search, then goes up a group, then quits.
  - Groups can't be created, renamed or deleted in kptui, but saving keeps them and the entries in them.
  - A vault without groups works just like before.
* **Hidden passwords**: In an entry, the password shows as dots until `Ctrl-r` shows it, and `Ctrl-r` hides it again.
  This works while editing the password too, and it's hidden again whenever an entry gets opened or closed.
* **Additional attributes**: The custom fields of an entry, like the "Additional attributes" in KeePassXC's
  Advanced section, or the extra fields of KeePass2Android. The Attributes line of an entry lists their names,
  and `Enter` on it opens them, with the selected value below the list:
  - `Enter` edits the value (which can have several lines), `Ctrl-n` adds an attribute,
    `Ctrl-e` renames it and `Ctrl-d` deletes it. With tmux copy on, `Ctrl-y` copies the value.
  - Protected values show as dots until `Ctrl-r` shows them. `Ctrl-p` protects or unprotects an attribute.
  - `Ctrl-s` saves the entry, and `Esc` goes back to it.
* **TOTP codes**: Generates live 2FA codes from a stored seed and saves `otpauth://` URI.
  - Editable as: `SECRET [DIGITS [PERIOD]]` (`PERIOD` defaults to `30`, and `DIGITS` to `6`)
* **Auto-exit**: The whole app can close itself automatically after a set period of inactivity
  in seconds (`0`: stays open indefinitely).
* **Themeable**: Ships with a range of themes and supports custom themes.
* **Remote vaults over SFTP**: The database can be an `sftp://[user@]host[:port]/path` URL, used through
  a local copy, like KeePass2Android does:
  - `host` can be an alias from `~/.ssh/config`, and a path starting with `/~/` is relative to the login directory.
    Spaces can be written as is, or as `%20`.
  - It uses the system's OpenSSH `sftp` in batch mode, so `~/.ssh/config`, keys, `ssh-agent` and `known_hosts` apply.
    Logging in must work without typing anything (a key without passphrase, or one in `ssh-agent`),
    and the server's host key must already be known.
  - On start, the server copy replaces the local copy (in `~/.local/share/kptui/remote/`).
    When the server can't be reached, the local copy gets opened instead.
  - Every save uploads the database. When the server copy changed in the meantime (another device saved),
    both get merged first, entry by entry and field by field. If the same field was changed differently,
    the newest version wins, and the other one is kept as an extra entry with " (conflict)" after its name.
    An entry deleted on one side but changed on the other is kept.
  - Changes that couldn't be uploaded stay in the local copy, and the Index title says "(not uploaded)".
    They get uploaded after the next unlock, or the next save, when the server can be reached.
  - Just opening and unlocking the database never writes to the server.
* **Copy to tmux (opt-in)**: With `tmux_copy = true` and kptui running inside tmux, `Ctrl-y` on an entry
  copies the selected field (or the current TOTP code) to a tmux paste buffer, to paste with `prefix ]`.
  Useful on a text console or over SSH, where there is no system clipboard.
  The buffer is deleted after `tmux_clear` seconds, or earlier when kptui exits.
* **Access Configuration from the app**: On the Login screen or the Index screen, give `Ctrl-c`:
  - Configuration file: `~/.config/kptui/config.toml`
  - Database (default: `~/.local/share/kptui/default.kdbx`, or an `sftp://` URL)
  - (Optional) keyfile
  - Auto-exit setting (in seconds, `0`: never auto-exit)
  - Theme selection (location: `~/.config/kptui/themes/`)
  - tmux copy on/off and the tmux buffer clear delay
  - Change the master password
  - Import from file
  - Export to file
* **Import from other vaults or export**: Import entries from another `.kdbx` file, or from a `.csv`/`.json` file
  (as produced by other password managers). Export to CSV/JSON or KDBX.

## Usage
```
kptui 0.39.0 - TUI password manager for KeePass vaults
Usage:  kptui [slim | version | help]
```

## Installing
### Download static single-binary `kptui` for amd64 Linux
```
wget https://github.com/pepa65/kptui/releases/download/0.39.0/kptui
sudo mv kptui /usr/local/bin
sudo chown root:root /usr/local/bin/kptui
sudo chmod +x /usr/local/bin/kptui
```

### Download static single-binary `kptui` for Android
```
wget -O kptui https://github.com/pepa65/kptui/releases/download/0.39.0/kptui_android
sudo chmod +x /usr/local/bin/kptui
```

### Download deb/rpm `kpgui` for amd64 Linux
```
wget https://github.com/pepa65/kptui/releases/download/0.39.0/kpgui_0.39.0-1_amd64.deb
wget https://github.com/pepa65/kptui/releases/download/0.39.0/kpgui_0.39.0-1.x86_64.rpm
```

### Install with cargo
#### Static musl build from cloned repo
```
# After git-cloning the repo
rustup target add x86_64-unknown-linux-musl
cargo build --release
```

For potentially much faster operation, replace `target-cpu=generic` with `target-cpu=native` in `.cargo/config.toml`!

#### Dynamic build with cargo
`cargo install --git https://github.com/pepa65/kptui`

### Install with cargo-binstall
Even without a full Rust toolchain, rust binaries can be installed with the static binary `cargo-binstall`:

```
# Install cargo-binstall for Linux x86_64
# (Other versions are available at https://crates.io/crates/cargo-binstall)
wget github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-musl.tgz
tar xf cargo-binstall-x86_64-unknown-linux-musl.tgz
sudo chown root:root cargo-binstall
sudo mv cargo-binstall /usr/local/bin/
```

Only a linux-x86_64 (musl) binary available: `cargo-binstall kptui`

It will be installed in `~/.cargo/bin/` which will need to be added to `PATH`!

## Getting started
* On first launch, if no vault is found at the default (configured) path, `kptui` will:
  - Prompt to create a new database.
  - Prompt to set a master password.
  - But if a password is piped or directed in, this password will be used to create a new database with that password.
  - If `Ctrl-c` is given, then the configuration can be adjusted:
    * `Ctrl-c` from the **Login screen**: adjust the configuration file
    * `Ctrl-c` from the **Index screen** (after decrypting):
      Also allow importing/exporting and re-encrypting the database with a different password or keyfile,
      or different KDF values for Itrerations and Memory.
* Once unlocked, entries can be browsed, searched and opened from the Index screen.
* To open an existing vault, set `database` in the config file at `~/.config/kptui/config.toml`.
  - It can also be set on the Configuration screen that's accessible with `Ctrl-c` from the Login screen.
  - If a keyfile is required, that can also be set in the Config screen with `Ctrl-c` in the Login screen.
```toml
# Example
database = "~/passwords.kdbx"
keyfile = ""  # Empty means: no keyfile is set
kdf_iterations = 90  # The default
kdf_memory = "256M"  # 256 MiB; also accepts a byte count (unquoted!)
```
* The interface requires a terminal dimensions of at least 44 colums by 24 rows (but 24x12 is still functional).
* Run in smaller terminals with: `kptui slim` (at least 27 columns by 10 rows required, but 19x11 is still functional).
* To show the version: `kptui version`
* To show a short help: `kptui help`
* A password can also be provided non-interactively:
  - Piped in, like: `echo "$password" |kptui`
  - Directed in, like: `kptui <<<"$password"`
  - Use a file with the password: `kptui <password_file`

## Configuration
The configuration file is at a fixed location: `~/.config/kptui/config.toml`.
See `default_config.toml` in this repo:
```toml
database = "~/.local/share/kptui/default.kdbx"  # Or a remote one, like "sftp://me@nas/~/vault.kdbx"
keyfile = "~/.local/share/kptui/default.keyx"  # Optional (empty for password-only vaults)
kdf_iterations = 90  # Can be lower if less paranoid about being brute-forced
kdf_memory = "256M"  # 256 MiB, also accepts byte count (unquoted!)
auto_exit = 300  # Seconds of inactivity before exiting the app
theme = "melange_dark.toml"  # Theme from ~/.local/share/kptui/themes
tmux_copy = false  # Inside tmux, Ctrl-y on an entry copies the selected field to a tmux paste buffer
tmux_clear = 30  # Seconds before tmux deletes the copied value (at least 1)
```

All fields are optional, as the above are the default values.

## Security notes
* Saved vaults are standard KDBX4.1 files, encrypted with the master password (and, when configured, the keyfile).
  - The keyfile is always in addition to the mandatory password, as an extra requirement.
  - If the configured keyfile is missing, unreadable, empty, or incorrect, an explicit error is given.
  - The keyfile's path is stored in the config, never its contents.
* Passwords can be piped in or entered in the UI, max.length 1024 bytes of UTF-8.
* Changing the master password re-encrypts the whole vault in-place, and requires entering the **current** password first.
  (If a keyfile was set, it remains active.)
* Changing the keyfile re-encrypts the whole vault in-place.
* The app exits after `auto_exit` seconds of inactivity, clearing all decrypted entries and the master password from memory.
  All edits and changes will be lost (0: never auto-exit).
* When the app is open and the vault not locked, secrets are kept in memory!
* When the app exits, there are no plaintext secrets left anywhere in memory.
* With an `sftp://` database, only the encrypted database file is transferred, never the master password or keyfile:
  - Transfers use OpenSSH's `sftp` with `BatchMode=yes`: it never prompts, and an unknown host key makes it fail.
  - The URL can't hold a password, and hosts, users and paths that could pass as options are refused.
  - The local copy and the last uploaded version (next to it, with `.base` after the name) are kept in
    `~/.local/share/kptui/remote/`, which is only accessible to the user.
  - An upload goes to a hidden file next to the database (`.NAME.kptui-upload`), which then gets renamed over it,
    so the server file never ends up half-written. The database keeps its permissions on the server.
  - Like any save by kptui, an upload only keeps what kptui stores (see above). Attachments, entry history,
    icons and tags are not kept, so other clients sharing the database lose them too.
* Copying to tmux is off by default. When enabled, a copied value lives in the tmux server's memory until it is deleted:
  - The value is piped into `tmux load-buffer -`, so it never shows up in process arguments or environments.
  - kptui keeps track of a single buffer: copying again replaces it, and only that buffer is ever deleted.
  - Every copy has its deletion after `tmux_clear` seconds scheduled in the tmux server (`run-shell -d`), so it
    also happens if kptui gets killed or its pane is closed; on a normal exit kptui deletes the buffer right away.
    This needs tmux 3.2 or newer: with an older tmux, nothing is copied.
  - If another buffer gets created at the very same moment, kptui can't tell which one is its own:
    it says so and leaves both alone.
* Passwords are hidden on screen unless shown with `Ctrl-r`, and hidden again for every entry that gets opened.
  In the GUI, a hidden password can't be selected with the mouse, so it can't end up in the Primary selection unseen.
* Additional attributes keep their protection when saved, and protected values are hidden on screen until shown with `Ctrl-r`.
  The Attributes line of an entry only shows their names.
* Using encrypted swap or zram (or no swap) increases security.
* Booting with kernel cmdline option `init_on_free=1` causes every freed page to get zeroed.
* On Linux, some extra hardening takes place in the operation of kptui.
* A GUI version of kptui is available on Linux, with the idea that no potentially secret text strings are somehow left (on ratatui or in screen buffers), because the GUI can be controlled completely by the app, so zeroizing can be more complete, at most leaving pixels in the shared buffer of the compositor (which Wayland completely wipes). Both X11 and Wayland zeroizes their text assembly buffers and their Cairo image on drop.
  - There is an issue with entering passwords in X11 (not just for `kpgui` or `kptui`, for any application!) that any process running on the same X-server (or as the same user, because that process could get access to the same X-server) can in principle access any keystroke, so any other GUI app that is running at the same time. Wayland does not have this issue. So beware what you have running!
  - Using `kptui` on a text console does enhance security.

### Optional Linux GUI
The GNU/Linux `gui` feature opens the existing vault interface on native Wayland or X11, using the same configuration and vault operations.
* It uses PangoCairo for text shaping and supports XKB layout switching.
* Besides the keyboard shortcuts, field labels in the Config, Index or Entry are left-click selectable, and double-click activated.
* The font size can be changed by resizing the `kpgui` window (the last size is saved to `~/.config/kptui/gui-size`).
* To select text, mouse-drag select works, as well as double-click to select a whole line (both of these copy to the Primary clipboard, which can usually be pasted with middle-click). This operation is safer on Wayland!
* With no arguments, `kpgui` opens the GUI (an x86-64 gnu Linux build of kptui will accept `gui` as argument to open it). It uses native Wayland when `WAYLAND_DISPLAY` or `WAYLAND_SOCKET` is set, and X11 otherwise. Set environment variables `KPGUI_BACKEND=x11` or `KPGUI_BACKEND=wayland` to choose explicitly. Use `kpgui tui` for the full terminal UI, `kpgui slim` for the compact UI, or `kpgui help` and `kpgui version`.

#### Building the GUI binary (dynamically linked)
To build the GUI (feature: "gui") on Debian/Ubuntu, install `pkg-config`, `libx11-dev`, `libxkbcommon-dev`, `libpango1.0-dev` (Pango 1.44 or newer), `libcairo2-dev`, and from an x86-64 glibc Linux desktop, run: `cargo gui` to build it.

#### Building release packages
Run `./buildpkgs` from the checkout on Debian or Ubuntu to build both `.deb` and `.rpm` packages in `dist/`. It builds `kpgui` once and requires the GUI build dependencies above, plus Python 3, `dpkg-dev`, and `rpm`. The RPM uses the shared libraries available on the build host, so verify it on each target distribution before publishing.
