#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    description: String,
    completed: bool,
}

impl Task {
    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }
}

#[derive(Debug, Default)]
pub struct TaskList {
    tasks: Vec<Task>,
}

impl TaskList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, description: String) {
        if description.trim().is_empty() {
            return;
        }

        self.tasks.push(Task {
            description,
            completed: false,
        });
    }

    pub fn edit(&mut self, index: usize, description: String) -> bool {
        let Some(task) = self.tasks.get_mut(index) else {
            return false;
        };

        if description.trim().is_empty() {
            return false;
        }

        task.description = description;
        true
    }

    pub fn complete(&mut self, index: usize) -> bool {
        let Some(task) = self.tasks.get_mut(index) else {
            return false;
        };

        task.completed = true;
        true
    }

    pub fn uncomplete(&mut self, index: usize) -> bool {
        let Some(task) = self.tasks.get_mut(index) else {
            return false;
        };

        task.completed = false;
        true
    }

    pub fn pending(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter().filter(|task| !task.completed)
    }

    pub fn completed(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter().filter(|task| task.completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_task_list_is_empty() {
        let tasks = TaskList::new();

        assert_eq!(tasks.pending().count(), 0);
        assert_eq!(tasks.completed().count(), 0);
    }

    #[test]
    fn add_creates_a_pending_task() {
        let mut tasks = TaskList::new();

        tasks.add("Write Hello".to_string());

        let pending: Vec<_> = tasks.pending().collect();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].description(), "Write Hello");
        assert!(!pending[0].is_completed());
    }

    #[test]
    fn add_ignores_blank_descriptions() {
        let mut tasks = TaskList::new();

        tasks.add("   ".to_string());

        assert_eq!(tasks.pending().count(), 0);
    }

    #[test]
    fn edit_changes_a_task_description() {
        let mut tasks = TaskList::new();
        tasks.add("Learn Rust".to_string());

        assert!(tasks.edit(0, "Build pomock".to_string()));

        assert_eq!(
            tasks.pending().next().unwrap().description(),
            "Build pomock"
        );
    }

    #[test]
    fn edit_rejects_an_unknown_index() {
        let mut tasks = TaskList::new();

        assert!(!tasks.edit(0, "Missing".to_string()));
    }

    #[test]
    fn edit_rejects_a_blank_description() {
        let mut tasks = TaskList::new();
        tasks.add("Keep me".to_string());

        assert!(!tasks.edit(0, " ".to_string()));
        assert_eq!(tasks.pending().next().unwrap().description(), "Keep me");
    }

    #[test]
    fn complete_moves_a_task_to_completed() {
        let mut tasks = TaskList::new();
        tasks.add("Finish session".to_string());

        assert!(tasks.complete(0));

        assert_eq!(tasks.pending().count(), 0);
        assert_eq!(tasks.completed().count(), 1);
    }

    #[test]
    fn uncomplete_moves_a_task_back_to_pending() {
        let mut tasks = TaskList::new();
        tasks.add("Finish session".to_string());
        tasks.complete(0);

        assert!(tasks.uncomplete(0));

        assert_eq!(tasks.pending().count(), 1);
        assert_eq!(tasks.completed().count(), 0);
    }

    #[test]
    fn completing_an_unknown_index_returns_false() {
        let mut tasks = TaskList::new();

        assert!(!tasks.complete(0));
        assert!(!tasks.uncomplete(0));
    }
}
