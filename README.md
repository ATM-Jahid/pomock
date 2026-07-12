# pomock

`pomock` is a Pomodoro timer and task workspace for the terminal,
built with [Ratatui](https://ratatui.rs/)
and [Crossterm](https://github.com/crossterm-rs/crossterm).

> [!IMPORTANT]
> pomock is in early development.
> The timer, in-memory task workflow, keyboard controls,
> and mouse controls work today.
> Configuration, persistence, and notifications are planned
> but are not implemented yet.

## Current features

- Focus and break countdown sessions
  with start, pause, resume, reset, and next-session controls.
- Editable to-do and completed-task lists.
- Keyboard and mouse navigation.

Tasks and the completed-focus count currently reset when pomock exits.

## Run from source

pomock currently targets Rust 2024 and requires a recent stable Rust toolchain.

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

### Clock

- `Space` — start a ready session, pause or resume an active session.
- `f` — select the next focus or break session without starting it.
- `r` — reset a running or paused session to its full duration.
- Double-click the clock — perform the same action as `Space`.

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

## License

Copyright (C) 2026 ATM Jahid Hasan<br>
**pomock** is released under the
[GNU AGPL](https://www.gnu.org/licenses/agpl-3.0.en.html).
