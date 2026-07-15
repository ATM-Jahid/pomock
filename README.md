# pomock

`pomock` is a Pomodoro timer and task workspace for the terminal,
built with [Ratatui](https://ratatui.rs/)
and [Crossterm](https://github.com/crossterm-rs/crossterm).

> [!IMPORTANT]
> `pomock` is in early development.
> The timer, in-memory task workflow, keyboard controls,
> and mouse controls work today.
> Task persistence, settings UI, and notifications are planned
> but are not implemented yet.

## Current features

- Focus and break countdown sessions
  with start, pause, resume, reset, and cycle-session controls.
- Editable to-do and completed-task lists.
- Keyboard and mouse navigation.
- TOML configuration for session durations and the long-break interval.

Tasks and the completed-focus count currently reset when `pomock` exits.

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
```

To customize them, create `pomock/config.toml`
in your platform's standard user configuration directory
(for example, `$XDG_CONFIG_HOME` or `~/.config` on Linux).
All duration values and `long_break_interval` must be greater than zero.

## License

Copyright (C) 2026 ATM Jahid Hasan<br>
`pomock` is released under the
[GNU AGPL](https://www.gnu.org/licenses/agpl-3.0.en.html).
