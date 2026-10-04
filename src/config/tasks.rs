/// Durable task behavior and presentation settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TasksConfig {
    show_numbers: bool,
    persist: bool,
}

impl TasksConfig {
    pub fn new(persist: bool) -> Self {
        Self::with_options(true, persist)
    }

    pub fn with_options(show_numbers: bool, persist: bool) -> Self {
        Self {
            show_numbers,
            persist,
        }
    }

    pub fn show_numbers(&self) -> bool {
        self.show_numbers
    }

    pub fn persist(&self) -> bool {
        self.persist
    }
}

impl Default for TasksConfig {
    fn default() -> Self {
        Self::with_options(true, true)
    }
}
