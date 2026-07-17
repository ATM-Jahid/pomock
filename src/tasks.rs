#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    description: String,
}

impl Task {
    pub(crate) fn new(description: String) -> Self {
        Self { description }
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}

/// Two independently ordered task lists matching the two workspace views.
#[derive(Debug, Clone, Default)]
pub struct TaskList {
    pending: Vec<Task>,
    completed: Vec<Task>,
}

impl TaskList {
    pub(crate) fn from_descriptions(pending: Vec<String>, completed: Vec<String>) -> Self {
        Self {
            pending: pending.into_iter().map(Task::new).collect(),
            completed: completed.into_iter().map(Task::new).collect(),
        }
    }

    pub fn add(&mut self, description: String) {
        Self::add_to(&mut self.pending, description);
    }

    pub fn add_completed(&mut self, description: String) {
        Self::add_to(&mut self.completed, description);
    }

    fn add_to(tasks: &mut Vec<Task>, description: String) {
        if !description.trim().is_empty() {
            tasks.push(Task::new(description));
        }
    }

    pub fn edit_pending(&mut self, index: usize, description: String) -> bool {
        Self::edit(&mut self.pending, index, description)
    }

    pub fn edit_completed(&mut self, index: usize, description: String) -> bool {
        Self::edit(&mut self.completed, index, description)
    }

    fn edit(tasks: &mut [Task], index: usize, description: String) -> bool {
        let Some(task) = tasks.get_mut(index) else {
            return false;
        };
        if description.trim().is_empty() {
            return false;
        }

        task.description = description;
        true
    }

    /// Moves a to-do task to the end of the done list.
    pub fn complete(&mut self, index: usize) -> bool {
        Self::move_to_end(&mut self.pending, &mut self.completed, index)
    }

    /// Moves a done task to the end of the to-do list.
    pub fn uncomplete(&mut self, index: usize) -> bool {
        Self::move_to_end(&mut self.completed, &mut self.pending, index)
    }

    fn move_to_end(source: &mut Vec<Task>, destination: &mut Vec<Task>, index: usize) -> bool {
        if index >= source.len() {
            return false;
        }

        destination.push(source.remove(index));
        true
    }

    pub fn delete_pending(&mut self, index: usize) -> bool {
        Self::delete(&mut self.pending, index)
    }

    pub fn delete_completed(&mut self, index: usize) -> bool {
        Self::delete(&mut self.completed, index)
    }

    fn delete(tasks: &mut Vec<Task>, index: usize) -> bool {
        if index >= tasks.len() {
            return false;
        }

        tasks.remove(index);
        true
    }

    pub fn pending(&self) -> impl Iterator<Item = &Task> {
        self.pending.iter()
    }

    pub fn completed(&self) -> impl Iterator<Item = &Task> {
        self.completed.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptions<'a>(tasks: impl Iterator<Item = &'a Task>) -> Vec<&'a str> {
        tasks.map(Task::description).collect()
    }

    #[test]
    fn new_task_list_is_empty() {
        let tasks = TaskList::default();

        assert_eq!(tasks.pending().count(), 0);
        assert_eq!(tasks.completed().count(), 0);
    }

    #[test]
    fn add_appends_to_the_chosen_list_and_ignores_blanks() {
        let mut tasks = TaskList::default();

        tasks.add("To do".to_string());
        tasks.add_completed("Done".to_string());
        tasks.add("   ".to_string());

        assert_eq!(descriptions(tasks.pending()), ["To do"]);
        assert_eq!(descriptions(tasks.completed()), ["Done"]);
    }

    #[test]
    fn editing_is_local_to_each_list() {
        let mut tasks =
            TaskList::from_descriptions(vec!["Pending".to_string()], vec!["Completed".to_string()]);

        assert!(tasks.edit_pending(0, "New pending".to_string()));
        assert!(tasks.edit_completed(0, "New completed".to_string()));
        assert!(!tasks.edit_pending(1, "Missing".to_string()));
        assert!(!tasks.edit_completed(0, " ".to_string()));

        assert_eq!(descriptions(tasks.pending()), ["New pending"]);
        assert_eq!(descriptions(tasks.completed()), ["New completed"]);
    }

    #[test]
    fn complete_appends_the_task_to_the_completed_list() {
        let mut tasks = TaskList::from_descriptions(
            vec!["First".to_string(), "Move me".to_string()],
            vec!["Already done".to_string()],
        );

        assert!(tasks.complete(1));

        assert_eq!(descriptions(tasks.pending()), ["First"]);
        assert_eq!(descriptions(tasks.completed()), ["Already done", "Move me"]);
    }

    #[test]
    fn uncomplete_appends_the_task_to_the_pending_list() {
        let mut tasks = TaskList::from_descriptions(
            vec!["Already pending".to_string()],
            vec!["Return me".to_string(), "Stay done".to_string()],
        );

        assert!(tasks.uncomplete(0));

        assert_eq!(
            descriptions(tasks.pending()),
            ["Already pending", "Return me"]
        );
        assert_eq!(descriptions(tasks.completed()), ["Stay done"]);
    }

    #[test]
    fn unknown_moves_do_not_change_either_list() {
        let mut tasks =
            TaskList::from_descriptions(vec!["Pending".to_string()], vec!["Completed".to_string()]);

        assert!(!tasks.complete(1));
        assert!(!tasks.uncomplete(1));

        assert_eq!(descriptions(tasks.pending()), ["Pending"]);
        assert_eq!(descriptions(tasks.completed()), ["Completed"]);
    }

    #[test]
    fn deletion_is_local_to_each_list() {
        let mut tasks = TaskList::from_descriptions(
            vec!["Keep".to_string(), "Delete pending".to_string()],
            vec!["Delete completed".to_string()],
        );

        assert!(tasks.delete_pending(1));
        assert!(tasks.delete_completed(0));
        assert!(!tasks.delete_pending(2));

        assert_eq!(descriptions(tasks.pending()), ["Keep"]);
        assert!(tasks.completed().next().is_none());
    }
}
