# pomock

`pomock` is a Pomodoro timer and task workspace for the terminal,
built with [Ratatui](https://ratatui.rs/)
and [Crossterm](https://github.com/crossterm-rs/crossterm).

> [!IMPORTANT]
> `pomock` is in early development.

## Current features

- Focus and break countdown sessions
  with start, pause, resume, reset, and cycle-session controls.
- Editable to-do and completed-task lists.
- Persistent task order and completion state.
- Keyboard and mouse navigation.
- Native desktop notifications when a session completes.
- A user-selected sound file when a session completes.
- TOML configuration for session durations, task behavior, and theme colors.
- An in-app settings overlay for timer, task, theme, and key settings.

The completed-focus count remains runtime-only and resets when `pomock` exits.

## Run from source

`pomock` currently targets Rust 2024 and requires a recent stable Rust toolchain.

```sh
git clone https://github.com/ATM-Jahid/pomock.git
cd pomock
cargo run
```

## Controls

Controls are contextual to the focused box.

### Global and box navigation

- `H`, `J`, `K`, `L` — move focus
  between the clock, to-do, and completed-task boxes.
- `q` — quit when not adding or editing a task.
- `s` — open the settings overlay.
Running or paused sessions ask for confirmation after 10 seconds of progress.

### Settings

- Up / Down or `j` / `k` — select a setting.
- Left / Right or `h` / `l` — adjust a number, toggle, or color.
- Enter or Space — edit a number, capture a key, toggle a value, or activate
  Save/Cancel.
- `s` — save settings and close the overlay.
- Esc — cancel the current edit/capture, or close the overlay without saving.
- Click a visible setting — select it; double-click to edit or activate it.

Opening settings pauses a running timer.
Canceling restores its prior activity.
Pressing `s` again saves and closes settings.
Saved changes are written immediately to `config.toml`.

### Clock

- `Space` — start a ready session, pause or resume an active session.
- `c` — cycle the session through focus, short break, and long break.
- `r` — reset a running or paused session to its full duration.
- Double-click the clock — perform the same action as `Space`.
- Click a different session button — change to that session.
- Double-click the active session button — pause or resume it.
- Double-click a different session button — change to that session and start it.

### To-do tasks

- `j` / `k` or Down / Up — move the selected row.
- `a` — add a task.
- `e` — edit the selected task.
- `x` — delete the selected task.
- `Space` — move the selected task to completed.
- Click a visible row — focus the to-do box and select that row.
- Double-click a visible row — complete that task.

### Completed tasks

- `j` / `k` or Down / Up — move the selected row.
- `e` — edit the selected task.
- `x` — delete the selected task.
- `Space` — return the selected task to to-do.
- Click a visible row — focus the completed-task box and select that row.
- Double-click a visible row — return that task to to-do.

While adding or editing, type normally, press Enter to submit,
or press Esc to cancel.
Mouse input is ignored until text entry finishes.

## Configuration

On first run,
`pomock` uses these defaults without requiring a configuration file:

```toml
[timer]
focus_minutes = 25
short_break_minutes = 5
long_break_minutes = 15
long_break_interval = 4

[tasks]
persist = true
show_numbers = true

[theme]
focused_border = "yellow"
unfocused_border = "dark_gray"
todo_highlight = "yellow"
done_highlight = "green"
completed_sessions = "green"

[keys]
focus_left = "H"
focus_down = "J"
focus_up = "K"
focus_right = "L"
list_down = ["j", "down"]
list_up = ["k", "up"]
quit = "q"
clock_primary = "space"
cycle_session = "c"
reset_session = "r"
add_task = "a"
edit_task = "e"
delete_task = "x"
task_primary = "space"
```

Sound is disabled by default. To play a custom file whenever any session
completes, add its path:

```toml
[sound]
file = "~/Music/session-complete.mp3"
```

Playback uses the system's default audio output and supports common formats,
including WAV, MP3, FLAC, Ogg Vorbis, and AAC.
An unreadable or unsupported file, or an unavailable audio device,
is ignored so the timer keeps running.
Paths beginning with `~/` are expanded from the current user's home directory.
Absolute paths are also accepted, but relative paths are not.

To customize them, create `pomock/config.toml`
in your platform's standard user configuration directory
(for example, `$XDG_CONFIG_HOME` or `~/.config` on Linux).
All duration values and `long_break_interval` must be greater than zero.

By default, `pomock` saves task descriptions, order, and completion state
after every successful task change.
The `tasks.toml` file lives under
the platform's standard per-user application data directory
(for example, `$XDG_DATA_HOME/pomock` or `~/.local/share/pomock` on Linux).

The `[theme]` section is optional, and individual omitted roles keep their
defaults. Colors use portable terminal names: `black`, `red`, `green`,
`yellow`, `blue`, `magenta`, `cyan`, `gray`, `dark_gray`, the `light_` variants
of red through cyan, and `white`.

The `[keys]` section is also optional.
Each binding is either a single key or an ordered list of keys;
the controls text shows only the first key.
A key is one printable character or one of
`space`, `enter`, `esc`, `backspace`, `up`, `down`, `left`, and `right`.

## License

Copyright (C) 2026 ATM Jahid Hasan<br>
`pomock` is released under the
[GNU AGPL](https://www.gnu.org/licenses/agpl-3.0.en.html).
