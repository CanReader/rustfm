use std::{
    path::{Path, PathBuf},
    sync::mpsc::{channel, Receiver, Sender},
    thread,
};

use crate::fs_ops;

#[derive(Debug)]
pub enum Task {
    Copy { id: u64, sources: Vec<PathBuf>, dest_dir: PathBuf },
    Move { id: u64, sources: Vec<PathBuf>, dest_dir: PathBuf },
    Delete { id: u64, targets: Vec<PathBuf>, use_trash: bool },
}

impl Task {
    pub fn id(&self) -> u64 {
        match self {
            Task::Copy { id, .. } | Task::Move { id, .. } | Task::Delete { id, .. } => *id,
        }
    }
}

#[derive(Debug)]
pub enum TaskMsg {
    Start { id: u64, label: String, total: u64 },
    Progress { id: u64, done: u64, current: String },
    Done { id: u64, ok: usize, errs: usize, first_error: Option<String> },
}

pub struct Worker {
    task_tx: Sender<Task>,
    pub msg_rx: Receiver<TaskMsg>,
}

impl Worker {
    pub fn spawn() -> Self {
        let (task_tx, task_rx) = channel::<Task>();
        let (msg_tx, msg_rx) = channel::<TaskMsg>();
        thread::spawn(move || worker_loop(task_rx, msg_tx));
        Self { task_tx, msg_rx }
    }

    pub fn submit(&self, task: Task) {
        let _ = self.task_tx.send(task);
    }
}

fn worker_loop(rx: Receiver<Task>, tx: Sender<TaskMsg>) {
    while let Ok(task) = rx.recv() {
        match task {
            Task::Copy { id, sources, dest_dir } => {
                run_transfer(id, sources, dest_dir, &tx, false, "copy");
            }
            Task::Move { id, sources, dest_dir } => {
                run_transfer(id, sources, dest_dir, &tx, true, "move");
            }
            Task::Delete { id, targets, use_trash } => {
                let label = if use_trash { "trash" } else { "delete" };
                let total = targets.len() as u64;
                let _ = tx.send(TaskMsg::Start {
                    id,
                    label: label.into(),
                    total,
                });
                let mut ok = 0usize;
                let mut errs = 0usize;
                let mut first_error: Option<String> = None;
                for (i, t) in targets.iter().enumerate() {
                    let _ = tx.send(TaskMsg::Progress {
                        id,
                        done: i as u64,
                        current: display_name(t),
                    });
                    match fs_ops::delete_path(t, use_trash) {
                        Ok(_) => ok += 1,
                        Err(e) => {
                            errs += 1;
                            if first_error.is_none() {
                                first_error = Some(format!("{}: {e}", display_name(t)));
                            }
                        }
                    }
                }
                let _ = tx.send(TaskMsg::Done { id, ok, errs, first_error });
            }
        }
    }
}

fn run_transfer(
    id: u64,
    sources: Vec<PathBuf>,
    dest_dir: PathBuf,
    tx: &Sender<TaskMsg>,
    is_move: bool,
    label: &str,
) {
    let total = sources.len() as u64;
    let _ = tx.send(TaskMsg::Start {
        id,
        label: label.into(),
        total,
    });
    let mut ok = 0usize;
    let mut errs = 0usize;
    let mut first_error: Option<String> = None;
    for (i, src) in sources.iter().enumerate() {
        let Some(name) = src.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            errs += 1;
            if first_error.is_none() {
                first_error = Some(format!("{}: invalid name", src.display()));
            }
            continue;
        };
        let _ = tx.send(TaskMsg::Progress {
            id,
            done: i as u64,
            current: name.clone(),
        });
        let dst = fs_ops::unique_destination(&dest_dir, &name);
        let res = if is_move {
            fs_ops::move_path(src, &dst)
        } else {
            fs_ops::copy_path(src, &dst)
        };
        match res {
            Ok(inner) => {
                if inner.is_empty() {
                    ok += 1;
                } else {
                    // Partial: top-level item completed but had child errors.
                    // Count it as ok (the user sees the destination) but
                    // surface the first inner error so they know something
                    // inside was skipped.
                    ok += 1;
                    errs += inner.len();
                    if first_error.is_none() {
                        first_error = inner.into_iter().next();
                    }
                }
            }
            Err(e) => {
                errs += 1;
                if first_error.is_none() {
                    first_error = Some(format!("{name}: {e}"));
                }
            }
        }
    }
    let _ = tx.send(TaskMsg::Done { id, ok, errs, first_error });
}

fn display_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}
