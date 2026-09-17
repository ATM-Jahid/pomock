use std::{env, io};

use pomock::persistence::{ConfigStore, TaskStore};

use runtime::{TerminalSession, combine_run_and_restore_results, run_app, task_store_for_config};
use startup::{
    CliCommand, load_config_for_startup, load_tasks_for_startup, write_help, write_version,
};

mod runtime;
mod startup;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout();
    let command = CliCommand::parse(env::args_os().skip(1))?;
    let workspace_name = match command {
        CliCommand::Run { workspace } => workspace,
        CliCommand::Help => {
            write_help(&mut stdout)?;
            return Ok(());
        }
        CliCommand::Version => {
            write_version(&mut stdout)?;
            return Ok(());
        }
    };
    let workspace = runtime::Workspace {
        task_store: TaskStore::user_in_workspace(workspace_name.as_deref())?,
        config_store: ConfigStore::user_in_workspace(workspace_name.as_deref())?,
    };
    let _workspace_lock = workspace.task_store.lock_workspace()?;
    let main_config_store = ConfigStore::user()?;
    if workspace.config_store != main_config_store {
        workspace
            .config_store
            .create_workspace_file(&main_config_store)?;
    }
    let Some(config) = load_config_for_startup(&workspace.config_store, &mut stdin, &mut stdout)?
    else {
        return Ok(());
    };
    let task_store = task_store_for_config(&config, &workspace.task_store);
    let Some(task_state) = load_tasks_for_startup(task_store.as_ref(), &mut stdin, &mut stdout)?
    else {
        return Ok(());
    };
    let mut session = TerminalSession::start()?;
    let run_result = run_app(
        session.terminal_mut(),
        config,
        task_store,
        task_state,
        workspace,
    );
    let restore_result = session.restore();

    let write_errors = combine_run_and_restore_results(run_result, restore_result)?;
    for error in write_errors {
        eprintln!("{error}");
    }
    Ok(())
}

#[cfg(test)]
#[path = "main/tests.rs"]
mod tests;
