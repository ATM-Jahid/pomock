#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    description: String,
    completed: bool,
}

impl Task {
    pub(crate) fn new(description: String, completed: bool) -> Self {
        Self {
            description,
            completed,
        }
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn is_completed(&self) -> bool {
        self.completed
    }
}

#[derive(Debug, Clone, Default)]
pub struct TaskList {
    tasks: Vec<Task>,
}

impl TaskList {
    pub(crate) fn from_tasks(tasks: Vec<Task>) -> Self {
        Self { tasks }
    }

    pub(crate) fn all(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter()
    }

    pub fn add(&mut self, description: String) {
        if description.trim().is_empty() {
            return;
        }

        self.tasks.push(Task::new(description, false));
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

    pub fn delete(&mut self, index: usize) -> bool {
        if index >= self.tasks.len() {
            return false;
        }

        self.tasks.remove(index);
        true
    }

    pub fn pending(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter().filter(|task| !task.completed)
    }

    pub fn completed(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter().filter(|task| task.completed)
    }

    pub fn pending_with_indices(&self) -> impl Iterator<Item = (usize, &Task)> {
        self.tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| !task.completed)
    }

    pub fn completed_with_indices(&self) -> impl Iterator<Item = (usize, &Task)> {
        self.tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| task.completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_task_list_is_empty() {
        let tasks = TaskList::default();

        assert_eq!(tasks.pending().count(), 0);
        assert_eq!(tasks.completed().count(), 0);
    }

    #[test]
    fn add_creates_a_pending_task() {
        let mut tasks = TaskList::default();

        tasks.add("Write Hello".to_string());

        let pending: Vec<_> = tasks.pending().collect();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].description(), "Write Hello");
    }

    #[test]
    fn add_ignores_blank_descriptions() {
        let mut tasks = TaskList::default();

        tasks.add("   ".to_string());

        assert_eq!(tasks.pending().count(), 0);
    }

    #[test]
    fn edit_changes_a_task_description() {
        let mut tasks = TaskList::default();
        tasks.add("Learn Rust".to_string());

        assert!(tasks.edit(0, "Build pomock".to_string()));

        assert_eq!(
            tasks.pending().next().unwrap().description(),
            "Build pomock"
        );
    }

    #[test]
    fn edit_rejects_an_unknown_index() {
        let mut tasks = TaskList::default();

        assert!(!tasks.edit(0, "Missing".to_string()));
    }

    #[test]
    fn edit_rejects_a_blank_description() {
        let mut tasks = TaskList::default();
        tasks.add("Keep me".to_string());

        assert!(!tasks.edit(0, " ".to_string()));
        assert_eq!(tasks.pending().next().unwrap().description(), "Keep me");
    }

    #[test]
    fn complete_moves_a_task_to_completed() {
        let mut tasks = TaskList::default();
        tasks.add("Finish session".to_string());

        assert!(tasks.complete(0));

        assert_eq!(tasks.pending().count(), 0);
        assert_eq!(tasks.completed().count(), 1);
    }

    #[test]
    fn uncomplete_moves_a_task_back_to_pending() {
        let mut tasks = TaskList::default();
        tasks.add("Finish session".to_string());
        tasks.complete(0);

        assert!(tasks.uncomplete(0));

        assert_eq!(tasks.pending().count(), 1);
        assert_eq!(tasks.completed().count(), 0);
    }

    #[test]
    fn completing_an_unknown_index_returns_false() {
        let mut tasks = TaskList::default();

        assert!(!tasks.complete(0));
        assert!(!tasks.uncomplete(0));
    }

    #[test]
    fn delete_removes_a_task() {
        let mut tasks = TaskList::default();
        tasks.add("Keep".to_string());
        tasks.add("Delete".to_string());

        assert!(tasks.delete(1));
        assert!(!tasks.delete(3));

        let pending: Vec<_> = tasks.pending().collect();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].description(), "Keep");
    }

    #[test]
    fn pending_indices_refer_to_positions_in_the_full_list() {
        let mut tasks = TaskList::default();
        tasks.add("Completed first".to_string());
        tasks.add("Pending second".to_string());
        tasks.complete(0);

        let pending: Vec<_> = tasks.pending_with_indices().collect();

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, 1);
        assert_eq!(pending[0].1.description(), "Pending second");
    }

    #[test]
    fn completed_indices_refer_to_position_in_the_full_list() {
        let mut tasks = TaskList::default();
        tasks.add("Pending first".to_string());
        tasks.add("Completed second".to_string());
        tasks.complete(1);

        let completed: Vec<_> = tasks.completed_with_indices().collect();

        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].0, 1);
        assert_eq!(completed[0].1.description(), "Completed second");
    }
}
