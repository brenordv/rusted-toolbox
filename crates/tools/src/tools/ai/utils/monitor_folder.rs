use anyhow::Result;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::future::Future;
use std::path::Path;
use std::sync::mpsc;

pub async fn monitor_folder_for_new_files<Fut, F>(
    folder_to_watch: &str,
    on_created_handler: Option<F>,
) -> anyhow::Result<()>
where
    F: Fn(Event) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    monitor_folder(
        folder_to_watch,
        None,
        None,
        on_created_handler,
        None,
        None,
        None,
    )
    .await?;

    Ok(())
}

pub async fn monitor_folder<Fut, F>(
    folder_to_watch: &str,
    on_any_handler: Option<fn(Event) -> anyhow::Result<()>>,
    on_access_handler: Option<fn(Event) -> anyhow::Result<()>>,
    on_created_handler: Option<F>,
    on_modify_handler: Option<fn(Event) -> anyhow::Result<()>>,
    on_remove_handler: Option<fn(Event) -> anyhow::Result<()>>,
    on_other_handler: Option<fn(Event) -> anyhow::Result<()>>,
) -> anyhow::Result<()>
where
    F: Fn(Event) -> Fut,
    Fut: Future<Output = Result<()>>,
{
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
                    if let Some(handler) = on_created_handler.as_ref() {
                        handler(event).await?;
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
