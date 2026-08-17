use std::{env, io};

use pomock::persistence::TaskStore;

use runtime::{TerminalSession, combine_run_and_restore_results, run_app, task_store_for_config};
use startup::{CliCommand, load_config_for_startup, load_tasks_for_startup, write_help};

mod runtime;
mod startup;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout();
    let command = CliCommand::parse(env::args_os().skip(1))?;
    let CliCommand::Run { workspace } = command else {
        write_help(&mut stdout)?;
        return Ok(());
    };
    let workspace_store = TaskStore::user_in_workspace(workspace.as_deref())?;
    let _workspace_lock = workspace_store.lock_workspace()?;
    let Some(config) = load_config_for_startup(&mut stdin, &mut stdout)? else {
        return Ok(());
    };
    let task_store = task_store_for_config(&config, &workspace_store);
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
        workspace_store,
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
