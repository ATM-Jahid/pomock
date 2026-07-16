/// Durable task behavior and presentation settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TasksConfig {
    persist: bool,
    show_numbers: bool,
}

impl TasksConfig {
    pub fn new(persist: bool) -> Self {
        Self::with_numbering(persist, true)
    }

    pub fn with_numbering(persist: bool, show_numbers: bool) -> Self {
        Self {
            persist,
            show_numbers,
        }
    }

    pub fn persist(&self) -> bool {
        self.persist
    }

    pub fn show_numbers(&self) -> bool {
        self.show_numbers
    }
}

impl Default for TasksConfig {
    fn default() -> Self {
        Self::with_numbering(true, true)
    }
}
