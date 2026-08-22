# pomock

`pomock` is a Pomodoro timer and task workspace for the terminal,
built with [Ratatui](https://ratatui.rs/)
and [Crossterm](https://github.com/crossterm-rs/crossterm).

> [!IMPORTANT]
> `pomock` is in early development.

## ✨ Current features

- Focus, short break, and long break sessions.
- Editable to-do and done lists.
- Named task workspaces for running independent `pomock` instances.
- Keyboard and mouse navigation.
- Desktop notifications.
- Completion and looping Focus audio.
- An in-app settings overlay for all configurable options.
- TOML configuration for persistent settings.

## 📦 Installation

`pomock` currently targets Rust 2024 and requires a recent stable Rust toolchain.
If Rust is not installed, install it with [rustup](https://rustup.rs/).

```sh
git clone https://github.com/ATM-Jahid/pomock.git
cd pomock
cargo build --release
sudo install -Dm755 target/release/pomock /usr/local/bin/pomock
```

To run the program directly from the cloned repo without installing it, use:

```sh
cargo run
```

To use an independent named task workspace, pass `--wspace`:

```sh
pomock --wspace foo
# OR
cargo run -- --wspace foo
```

Named workspaces are created on first use.
Only one `pomock` instance can connect to a workspace at a time.

## 🎛️ Controls

Controls (keybindings and mouse actions) are contextual to the focused box.

### Global and box navigation

| Key | Action |
| --- | --- |
| `H`, `J`, `K`, `L` | Move focus between the clock, to-do, and done boxes. |
| `q` | Quit (asks for confirmation if a session is running). |
| `s` | Open or close the settings overlay. |
| `Esc` | Cancel pending autostart. |

### Clock

| Key | Action |
| --- | --- |
| `Space` | Start a ready session, pause or resume an active session. |
| `c` | Cycle the session through focus, short break, and long break. |
| `r` | Reset a running or paused session to its full duration. |

| Mouse target | 🖱×1 | 🖱×2 |
| --- | --- | --- |
| Clock | — | Perform the same action as `Space`. |
| Active session button | — | Pause or resume it. |
| Different session button | Change to that session. | Change to that session and start it. |

### To-do / Done

| Key | Action |
| --- | --- |
| `j` / `k` or ↓ / ↑ | Move selection. |
| `a` | Add a task. |
| `e` | Edit the selected task. |
| `x` | Delete the selected task. |
| `u` / `d` | Move the selected task up or down. |
| `Space` | Move the selected task to the other list. |

| Mouse target | 🖱×1 | 🖱×2 |
| --- | --- | --- |
| Visible row | Focus the box and select that row. | Move that task to the other list. |

While adding or editing,
press `Enter` to submit, or press `Esc` to cancel.
Mouse input is ignored until text entry finishes.

### Settings

| Key | Action |
| --- | --- |
| `j` / `k` or ↓ / ↑ | Select a setting. |
| `h` / `l` or ← / → | Adjust a number, toggle, or color. |
| `Enter` or `Space` | Edit the selected field. `Enter` also applies an entered value. |
| `s` | Close the overlay if in navigation mode. |
| `Esc` | Cancel the current edit or key capture before it is accepted. |

| Mouse target | 🖱×1 | 🖱×2 |
| --- | --- | --- |
| Visible setting | Select it. | Edit or activate it. |

Every accepted change takes effect and is written immediately to `config.toml`.

## ⚙️ Configuration

On first run,
`pomock` uses these defaults without requiring a configuration file:

```toml
[timer]
focus_duration = "25:00"
short_break_duration = "05:00"
long_break_duration = "15:00"
long_break_interval = 4
autostart_breaks = false
autostart_focus = false

[notification]
enabled = true

[sound.completion]
enabled = false

[sound.focus]
enabled = false

[tasks]
persist = true
show_numbers = true

[keys]
quit = "q"
settings = "s"
focus_left = "H"
focus_down = "J"
focus_up = "K"
focus_right = "L"
clock_primary = "space"
cycle_session = "c"
reset_session = "r"
add_task = "a"
edit_task = "e"
delete_task = "x"
task_primary = "space"
list_down = ["j", "down"]
list_up = ["k", "up"]
move_task_up = "u"
move_task_down = "d"

[theme]
focused_border = "light_red"
unfocused_border = "dark_gray"
focus = "magenta"
short_break = "cyan"
long_break = "green"
todo_highlight = "red"
done_highlight = "green"
```

On first startup, `pomock` creates `pomock/config.toml` with these defaults
in your platform's standard user configuration directory
(for example, `$XDG_CONFIG_HOME` or `~/.config` on Linux).
Edit that file to customize the settings (or use the in-app settings overlay).

By default, `pomock` saves task descriptions, order, and completion state
after every successful task change.
When task persistence is enabled,
`tasks.toml` is also created on startup
and lives under the platform's standard per-user application data directory
(for example, `$XDG_DATA_HOME/pomock` or `~/.local/share/pomock` on Linux).
For a named workspace, it lives in the workspace's child directory,
such as `~/.local/share/pomock/foo/tasks.toml`.

Each key binding is either a single key or an ordered list of keys;
the help text only shows the first key.
A key is one printable character or one of
`space`, `backspace`, `up`, `down`, `left`, and `right`.
You can prefix a non-character key
with any combination of `ctrl+`, `alt+`, and `shift+`.
Note that shifted printable keys
use the character produced by the terminal (`A` or `?`, for example),
without a `shift+` prefix.

Colors accept `#RRGGBB` values or portable terminal names:
`black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `gray`, `dark_gray`,
the `light_` variants of red through cyan, and `white`.

Native desktop notifications are enabled by default
and can be toggled with `notification.enabled`.
Completion audio is played once for a maximum of five seconds;
Focus audio loops only while a Focus session is running.
Both sound types are disabled and have no selected file by default.
Paths beginning with `~/` are expanded from the current user's home directory.
Other than that, only absolute paths are understood.

```toml
[sound.completion]
enabled = true
file = "~/Music/completion.mp3"

[sound.focus]
enabled = true
file = "~/Music/focus-ambience.wav"
```

## 📄 License

Copyright (C) 2026 ATM Jahid Hasan<br>
`pomock` is released under the
[GNU AGPL](https://www.gnu.org/licenses/agpl-3.0.en.html).
