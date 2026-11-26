[![version](https://img.shields.io/crates/v/kptui.svg)](https://crates.io/crates/kptui)
[![build](https://github.com/pepa65/kptui/actions/workflows/rust.yml/badge.svg)](https://github.com/pepa65/kptui/actions/workflows/rust.yml)
[![dependencies](https://deps.rs/repo/github/pepa65/kptui/status.svg)](https://deps.rs/repo/github/pepa65/kptui)
[![docs](https://img.shields.io/badge/docs-kptui-blue.svg)](https://docs.rs/crate/kptui/latest)
[![license](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://github.com/pepa65/kptui/blob/main/LICENSE.md)
[![downloads](https://img.shields.io/crates/d/kptui.svg)](https://crates.io/crates/kptui)

# kptui 0.45.0-nibblex.1
## About this fork
This is a fork of [pepa65/kptui](https://github.com/pepa65/kptui), which deliberately stays small.
It adds what a KeePassXC user on a text console or over SSH may miss:
* **Remote vaults over SFTP**: `database = "sftp://host/path/vault.kdbx"`, used through a local copy,
  with changes from other devices merged in when saving.
* **Groups**: the Index browses the groups (folders) of the vault.
* **Additional attributes**: the custom fields of KeePassXC's Advanced section, shown and editable.

To install it: `cargo install --git https://github.com/Nibblex/kptui --branch nibblex --locked kptui`,
or get a binary from the [releases](https://github.com/Nibblex/kptui/releases).
`kptui version` shows the fork's version, like `0.45.0-nibblex.1`. Everything below describes this fork,
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
* Integration with screen multiplexer **tmux** copy buffers is available in the configuration.

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
    `Ctrl-e` renames it and `Ctrl-d` deletes it. With Clear-tmux set, `Ctrl-c` copies the value to tmux.
  - Protected values show as dots until `Ctrl-r` shows them. `Ctrl-p` protects or unprotects an attribute.
  - `Ctrl-s` saves the entry, and `Esc` goes back to it.
* **TOTP codes**: Generates live 2FA codes (current and next, with countdown)
  from a stored seed and saves `otpauth://` URI.
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
  - Only one kptui on a device can use a remote vault at a time. Another one says so, and can try again with `Enter`.
  - Every save uploads the database. When the server copy changed in the meantime (another device saved),
    both get merged first, entry by entry and field by field. If the same field was changed differently,
    the newest version wins, and the other one is kept as an extra entry with " (conflict)" after its name.
    An entry deleted on one side but changed on the other is kept. Protecting an attribute counts as a change.
  - What another device saves during an upload gets merged and uploaded too, with one exception
    (see [Security notes](#security-notes)).
  - Changes that couldn't be uploaded stay in the local copy, and the Index title says "(not uploaded)".
    They get uploaded after the next unlock, or the next save, when the server can be reached.
  - Auto-exit doesn't wait for an upload: what isn't uploaded by then stays in the local copy.
  - Just opening and unlocking the database never writes to the server.
* **Access Configuration from the app**: On the Login screen or the Index screen, give `Ctrl-c`:
  - Default configfile: `~/.config/kptui/config.toml`
  - Database, or an `sftp://` URL [default: `~/.local/share/kptui/default.kdbx`]
  - (Optional) keyfile [default: ""]
  - KDF iterations [default: 90]
  - KDF memory [default: 256M]
  - Auto-exit setting (in seconds, `0`: never auto-exit) [default: 0]
  - Clear-tmux setting (number of seconds before clearing the copy buffer, 0: disabled)
  - Theme selection (themes directory: `~/.config/kptui/themes/`)
  - Save as KDBX4.x [x=0 when the database version is KDBX4.1, otherwise x=1] (only from Index screen)
  - Change the master password (only from Index screen)
  - Import from file (only from Index screen)
  - Export to file (only from Index screen)
* **Import from other vaults or export**: Import entries from another `.kdbx` file, or from a `.csv`/`.json` file
  (as produced by other password managers). Export to CSV/JSON or KDBX.
* **Documentation included**: The binaries include this full Readme, do: `kptui readme` or `kpgui -r`.
  Scrolling with arrows, page keys, Home/End works, and q/Esc/Ctrl-C closes.
* **Copy to tmux buffer**: When setting `Clear-tmux` on the Config screen (or `clear_tmux` in the configuration TOML file)
  to a value greater than 0, the selected field on the Entry screen gets copied to a **tmux** buffer with `Ctrl-c` and
  is retained for the set number of seconds. A value of `0` disables this feature. Hidden passwords can still be copied!
  Add this opt-in binding to `~/.tmux.conf` (and reload **tmux**):
	`bind-key C-v run-shell -C 'paste-buffer -p -d -b "#{@kptui-buffer}"'`
  Then TMUX_PREFIX followed by ^v will paste that buffer (could be a different key: replace `C-v`).
  This fork's earlier settings `tmux_copy` and `tmux_clear` still count when `clear_tmux` isn't set.

## Usage
```
kptui 0.45.0 - Highly secure TUI password manager for KeePass KDBX files (+GUI)
Usage: kptui [tui | slim | gui | readme | help | version] [NAME[.toml]]
  NAME[.toml] (default: config) selects configfile ~/.config/kptui/NAME.toml
```

`kpgui -r` opens the README in the GUI; `kptui -r` uses the terminal viewer.


## Installing
### Download static single-binary `kptui` for amd64 Linux
```
wget https://github.com/pepa65/kptui/releases/download/0.45.0/kptui
sudo mv kptui /usr/local/bin
sudo chown root:root /usr/local/bin/kptui
sudo chmod +x /usr/local/bin/kptui
```

### Download static single-binary `kptui` for Android
```
wget -O kptui https://github.com/pepa65/kptui/releases/download/0.45.0/kptui_android
sudo chmod +x /usr/local/bin/kptui
```

### Download deb/rpm `kpgui` for amd64 Linux
```
wget https://github.com/pepa65/kptui/releases/download/0.45.0/kpgui_0.45.0-1_amd64.deb
wget https://github.com/pepa65/kptui/releases/download/0.45.0/kpgui_0.45.0-1.x86_64.rpm
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
* To open an existing vault, set `database` in the configfile at `~/.config/kptui/config.toml`.
  - It can also be set on the Configuration screen that's accessible with `Ctrl-c` from the Login screen.
  - If a keyfile is required, that can also be set in the Config screen with `Ctrl-c` in the Login screen.
  - Alternative configfiles can be initiated/selected from the commandline: `kptui new` creates (or uses)
   `~/.config/kptui/new.toml`
* The interface requires a terminal dimensions of at least 44 colums by 24 rows (but 24x12 is still functional).
* Run in smaller terminals with: `kptui slim` (at least 27 columns by 10 rows required, but 19x11 is still functional).
* To show the version: `kptui version`
* To show a short help: `kptui help`
* A password can also be provided non-interactively:
  - Piped in, like: `echo "$password" |kptui`
  - Directed in, like: `kptui <<<"$password"`
  - Use a file with the password: `kptui <password_file`

## Configuration
The default configuration file is `~/.config/kptui/config.toml`.
Pass an optional filename (`.toml` is optional) to use another file in `~/.config/kptui/`, for example
`kpgui work`, `kptui slim slim.toml`, or `kpgui default.toml` (must follow the optional 'mode' command).
Missing files are created with default values, and configuration changes are saved to the selected file.
See `config.toml` in this repo:
```toml
database = "~/.local/share/kptui/default.kdbx"  # Or a remote one, like "sftp://me@nas/~/vault.kdbx"
keyfile = "~/.local/share/kptui/default.keyx"  # Optional (empty for password-only vaults)
kdf_iterations = 90  # Can be lower if less paranoid about being brute-forced
kdf_memory = "256M"  # 256 MiB, also accepts byte count (unquoted!)
auto_exit = 0  # Seconds of inactivity before exiting the app (0: never auto-exit)
clear_tmux = 0  # Disable using tmux copy buffers, a positive value enables and limits how long it's kept
theme = "default"  # Theme in ~/.local/share/kptui/themes/  # Optional
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
  - The local copy and other versions of the (encrypted) database are kept in `~/.local/share/kptui/remote/`,
    which is only accessible to the user: the last one known to be on the server (with `.base` after the name),
    and after an interrupted upload, what was being uploaded (`.uploading`) and what that replaces (`.replaced`).
  - An upload goes to a hidden file next to the database (`.NAME.kptui-upload-ID`, where ID is a random number for
    this device), which then gets renamed over it, so the server file never ends up half-written.
    The database keeps its permissions on the server, but not its owner.
  - SFTP can't lock a file, or replace it only if it didn't change. So right before the rename, in the same session,
    the server file is downloaded once more. If another device changed it, that gets merged and uploaded, too.
    If it can't be opened or is another database, it's kept next to the local copy (with `.unmerged-1` after the name),
    and the status says so. What can still get lost is a version that another program saves between that download
    and the rename, or that it's still writing at that moment.
  - Like any save by kptui, an upload only keeps what kptui stores (see above). Attachments, entry history,
    icons and tags are not kept, so other clients sharing the database lose them too.
* Passwords are hidden on screen unless shown with `Ctrl-r`, and hidden again for every entry that gets opened.
  In the GUI, a hidden password can be selected with the mouse too, so it can be copied without showing it.
* Additional attributes keep their protection when saved, and protected values are hidden on screen until shown with `Ctrl-r`.
  The Attributes line of an entry only shows their names.
* Using encrypted swap or zram (or no swap) increases security.
* Booting with kernel cmdline option `init_on_free=1` causes every freed page to get zeroed.
* On Linux, some extra hardening takes place in the operation of kptui.
* A GUI version of kptui is available on Linux, with the idea that no potentially secret text strings are somehow left (on ratatui or in screen buffers), because the GUI can be controlled completely by the app, so zeroizing can be more complete, at most leaving pixels in the shared buffer of the compositor (which Wayland completely wipes). Both X11 and Wayland zeroizes their text assembly buffers and their Cairo image on drop.
  - There is an issue with entering passwords in X11 (not just for `kpgui` or `kptui`, for any application!) that any process running on the same X-server (or as the same user, because that process could get access to the same X-server) can in principle access any keystroke, so any other GUI app that is running at the same time. Wayland does not have this issue. So beware what you have running!
  - Using `kptui` on a text console does enhance security.
* When using Clear-tmux, deleting a buffer does __not__ guarantee that **tmux** wipes its freed memory!

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
