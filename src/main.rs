use std::{
    env,
    io::{self, BufRead, Write},
};

use pomock::persistence::TaskStore;

use runtime::terminal::{TerminalSession, combine_run_and_restore_results};
use runtime::{effects::task_store_for_config, run_app};
use startup::cli::{CliCommand, write_help};
use startup::recovery::{load_config_for_startup, load_tasks_for_startup};

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
    let workspace_instance = workspace_store.register_instance()?;
    if workspace_instance.already_open()
        && !confirm_shared_workspace(workspace.as_deref(), &mut stdin, &mut stdout)?
    {
        return Ok(());
    }
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

    Ok(combine_run_and_restore_results(run_result, restore_result)?)
}

fn confirm_shared_workspace(
    workspace: Option<&str>,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> io::Result<bool> {
    let label = workspace.unwrap_or("default");
    writeln!(
        writer,
        "Warning: workspace {label:?} is already open. Multiple instances can overwrite each other's task changes."
    )?;

    loop {
        write!(writer, "Open it anyway? [y/N]: ")?;
        writer.flush()?;
        let mut choice = String::new();
        if reader.read_line(&mut choice)? == 0 {
            return Ok(false);
        }
        match choice.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "" | "n" | "no" => return Ok(false),
            _ => writeln!(writer, "Enter y to continue or n to quit.")?,
        }
    }
}

#[cfg(test)]
#[path = "main/tests.rs"]
mod tests;
