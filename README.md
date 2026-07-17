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
- Independently configurable native notifications, completion audio,
  and looping Focus audio.
- TOML configuration for session durations, task behavior, and theme colors.
- An in-app settings overlay for timer, notification, sound, task, key,
  and theme settings.

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
  Running or paused sessions ask for confirmation after 10 seconds of progress.
- `s` or `Esc` — open the settings overlay.

### Settings

- Up / Down or `j` / `k` — select a setting.
- Left / Right or `h` / `l` — adjust a number, toggle, or color.
- Enter or Space — edit the selected field.
  Enter applies an entered value;
  pressing a valid key applies a captured binding.
  Both return to navigation.
- The configured Settings key — close the overlay while navigating it.
- Esc — cancel the current edit or key capture before it is accepted.
- Click a visible setting — select it; double-click to edit or activate it.

Every accepted change takes effect and is written immediately to `config.toml`.

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

[notification]
enabled = true

[sound.completion]
enabled = false

[sound.focus]
enabled = false

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
settings = "s"
clock_primary = "space"
cycle_session = "c"
reset_session = "r"
add_task = "a"
edit_task = "e"
delete_task = "x"
task_primary = "space"
```

To customize them, create `pomock/config.toml`
in your platform's standard user configuration directory
(for example, `$XDG_CONFIG_HOME` or `~/.config` on Linux).
All duration values and `long_break_interval` must be greater than zero.

By default, `pomock` saves task descriptions, order, and completion state
after every successful task change.
The `tasks.toml` file lives under
the platform's standard per-user application data directory
(for example, `$XDG_DATA_HOME/pomock` or `~/.local/share/pomock` on Linux).

The `[theme]` section is optional.
Colors accept `#RRGGBB` values or portable terminal names:
`black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `gray`, `dark_gray`,
the `light_` variants of red through cyan, and `white`.

The `[keys]` section is also optional.
Each binding is either a single key or an ordered list of keys;
the controls text shows only the first key.
A key is one printable character or one of
`space`, `enter`, `backspace`, `up`, `down`, `left`, and `right`.
`Esc` is reserved as the fixed Settings alias in normal mode
and for cancel/back behavior in modal contexts.

Native desktop notifications are enabled by default
and can be toggled with `notification.enabled`.
Both sound types are disabled and have no selected file by default.
Completion audio is a one-shot effect;
Focus audio loops only while a Focus session is running.

```toml
[sound.completion]
enabled = true
file = "~/Music/completion.mp3"

[sound.focus]
enabled = true
file = "~/Music/focus-ambience.wav"
```

The Focus loop starts and resumes with a running Focus timer, pauses with it,
and stops on reset, session change, completion, disablement, or quit.

Playback uses the system's default audio output and supports common formats,
including WAV, MP3, FLAC, Ogg Vorbis, and AAC.
An unreadable or unsupported file, or an unavailable audio device,
is ignored so the timer keeps running.
Paths beginning with `~/` are expanded from the current user's home directory.
Absolute paths are also accepted, but relative paths are not.

## License

Copyright (C) 2026 ATM Jahid Hasan<br>
`pomock` is released under the
[GNU AGPL](https://www.gnu.org/licenses/agpl-3.0.en.html).
