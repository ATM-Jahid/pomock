use super::{App, Direction, EditMode, UiFocus};

impl UiFocus {
    pub(super) fn navigate(self, direction: Direction) -> Self {
        match (self, direction) {
            (Self::Clock, Direction::Down) => Self::Todo,
            (Self::Todo | Self::Done, Direction::Up) => Self::Clock,
            (Self::Todo, Direction::Right) => Self::Done,
            (Self::Done, Direction::Left) => Self::Todo,
            _ => self,
        }
    }
}

impl App {
    pub(super) fn focus(&mut self, focus: UiFocus) {
        self.ui_focus = focus;
        if matches!(focus, UiFocus::Todo | UiFocus::Done) {
            self.last_task_focus = focus;
        }
    }

    pub(super) fn navigate_focus(&mut self, direction: Direction) {
        self.focus(self.ui_focus.navigate(direction));
    }

    pub(super) fn select_todo(&mut self, selection: usize) {
        self.todo_selection = selection;
    }

    pub(super) fn select_done(&mut self, selection: usize) {
        self.done_selection = selection;
    }

    pub(super) fn begin_add(&mut self) {
        if !matches!(self.ui_focus, UiFocus::Todo | UiFocus::Done) {
            return;
        }

        self.input.clear();
        self.edit_mode = EditMode::Adding;
    }

    pub(super) fn cancel_edit(&mut self) {
        self.input.clear();
        self.edit_mode = EditMode::Normal;
    }

    pub(super) fn submit_edit(&mut self) -> bool {
        let description = std::mem::take(&mut self.input);

        let changed = match self.edit_mode {
            EditMode::Adding if !description.trim().is_empty() => {
                if self.ui_focus == UiFocus::Done {
                    self.tasks.add_completed(description);
                } else {
                    self.tasks.add(description);
                }
                true
            }
            EditMode::Editing { task_index } => match self.ui_focus {
                UiFocus::Todo => self.tasks.edit_pending(task_index, description),
                UiFocus::Done => self.tasks.edit_completed(task_index, description),
                UiFocus::Clock => false,
            },
            EditMode::Adding | EditMode::Normal => false,
        };

        self.edit_mode = EditMode::Normal;
        self.clamp_selections();
        changed
    }

    pub(super) fn push_input(&mut self, character: char) {
        self.input.push(character);
    }

    pub(super) fn pop_input(&mut self) {
        self.input.pop();
    }

    pub(super) fn move_todo_selection(&mut self, direction: Direction) {
        let len = self.tasks.pending().count();
        Self::move_selection(&mut self.todo_selection, len, direction);
    }

    pub(super) fn move_done_selection(&mut self, direction: Direction) {
        let len = self.tasks.completed().count();
        Self::move_selection(&mut self.done_selection, len, direction);
    }

    pub(super) fn move_selected_task(&mut self, direction: Direction) -> bool {
        match (self.ui_focus, direction) {
            (UiFocus::Todo, Direction::Up) => {
                let changed = self.tasks.move_pending_up(self.todo_selection);
                if changed {
                    self.todo_selection -= 1;
                    self.todo_offset = self.todo_offset.min(self.todo_selection);
                }
                changed
            }
            (UiFocus::Todo, Direction::Down) => {
                let changed = self.tasks.move_pending_down(self.todo_selection);
                if changed {
                    self.todo_selection += 1;
                }
                changed
            }
            (UiFocus::Done, Direction::Up) => {
                let changed = self.tasks.move_completed_up(self.done_selection);
                if changed {
                    self.done_selection -= 1;
                    self.done_offset = self.done_offset.min(self.done_selection);
                }
                changed
            }
            (UiFocus::Done, Direction::Down) => {
                let changed = self.tasks.move_completed_down(self.done_selection);
                if changed {
                    self.done_selection += 1;
                }
                changed
            }
            (UiFocus::Clock, _) | (_, Direction::Left | Direction::Right) => false,
        }
    }

    pub(super) fn edit_selected_todo(&mut self) {
        let description = self
            .tasks
            .pending()
            .nth(self.todo_selection)
            .map(|task| task.description().to_string());
        if let Some(description) = description {
            self.begin_edit(self.todo_selection, description);
        }
    }

    pub(super) fn edit_selected_done(&mut self) {
        let description = self
            .tasks
            .completed()
            .nth(self.done_selection)
            .map(|task| task.description().to_string());
        if let Some(description) = description {
            self.begin_edit(self.done_selection, description);
        }
    }

    pub(super) fn delete_selected_todo(&mut self) -> bool {
        if self.tasks.pending().nth(self.todo_selection).is_some() {
            let changed = self.tasks.delete_pending(self.todo_selection);
            self.clamp_selections();
            return changed;
        }
        false
    }

    pub(super) fn delete_selected_done(&mut self) -> bool {
        if self.tasks.completed().nth(self.done_selection).is_some() {
            let changed = self.tasks.delete_completed(self.done_selection);
            self.clamp_selections();
            return changed;
        }
        false
    }

    pub(super) fn complete_selected_todo(&mut self) -> bool {
        if self.tasks.pending().nth(self.todo_selection).is_some() {
            let changed = self.tasks.complete(self.todo_selection);
            self.clamp_selections();
            return changed;
        }
        false
    }

    pub(super) fn return_selected_done(&mut self) -> bool {
        if self.tasks.completed().nth(self.done_selection).is_some() {
            let changed = self.tasks.uncomplete(self.done_selection);
            self.clamp_selections();
            return changed;
        }
        false
    }

    fn move_selection(selection: &mut usize, len: usize, direction: Direction) {
        if len == 0 {
            *selection = 0;
            return;
        }

        match direction {
            Direction::Left | Direction::Up => {
                *selection = selection.saturating_sub(1);
            }
            Direction::Down | Direction::Right => {
                *selection = (*selection + 1).min(len - 1);
            }
        }
    }

    pub(super) fn clamp_selections(&mut self) {
        let pending_len = self.tasks.pending().count();
        let completed_len = self.tasks.completed().count();
        self.todo_selection = self.todo_selection.min(pending_len.saturating_sub(1));
        self.done_selection = self.done_selection.min(completed_len.saturating_sub(1));
        self.todo_offset = self.todo_offset.min(self.todo_selection);
        self.done_offset = self.done_offset.min(self.done_selection);
    }

    fn begin_edit(&mut self, task_index: usize, description: String) {
        self.input = description;
        self.edit_mode = EditMode::Editing { task_index };
    }
}
