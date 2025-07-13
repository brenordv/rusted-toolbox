use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;

pub fn monitor_folder_for_new_files(
    folder_to_watch: &str,
    on_created_handler: Option<fn(Event) -> anyhow::Result<()>>,
) -> anyhow::Result<()> {
    monitor_folder(
        folder_to_watch,
        None,
        None,
        on_created_handler,
        None,
        None,
        None,
    )?;

    Ok(())
}

pub fn monitor_folder(
    folder_to_watch: &str,
    on_any_handler: Option<fn(Event) -> anyhow::Result<()>>,
    on_access_handler: Option<fn(Event) -> anyhow::Result<()>>,
    on_created_handler: Option<fn(Event) -> anyhow::Result<()>>,
    on_modify_handler: Option<fn(Event) -> anyhow::Result<()>>,
    on_remove_handler: Option<fn(Event) -> anyhow::Result<()>>,
    on_other_handler: Option<fn(Event) -> anyhow::Result<()>>,
) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();

    let mut watcher = notify::recommended_watcher(tx)?;

    watcher.watch(Path::new(folder_to_watch), RecursiveMode::Recursive)?;

    for res in rx {
        match res {
            Ok(event) => match event.kind {
                EventKind::Any => {
                    if let Some(handler) = on_any_handler {
                        handler(event)?;
                    }
                }
                EventKind::Access(_) => {
                    if let Some(handler) = on_access_handler {
                        handler(event)?;
                    }
                }
                EventKind::Create(_) => {
                    if let Some(handler) = on_created_handler {
                        handler(event)?;
                    }
                }
                EventKind::Modify(_) => {
                    if let Some(handler) = on_modify_handler {
                        handler(event)?;
                    }
                }
                EventKind::Remove(_) => {
                    if let Some(handler) = on_remove_handler {
                        handler(event)?;
                    }
                }
                EventKind::Other => {
                    if let Some(handler) = on_other_handler {
                        handler(event)?;
                    }
                }
            },
            Err(e) => println!("watch error: {:?}", e),
        }
    }

    Ok(())
}
