//! Browser impl

// Prevent console window in addition to Slint window in Windows release builds
// when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{collections::HashMap, io, num::NonZeroUsize, process::Termination, rc::Rc};

mod ui;

mod args;

use args::Args;
use clap::Parser;
use shr::{Event, EventRef, PathId, ShrRx, utils::human_readable_number};
use slint::{ComponentHandle, LogicalSize, SharedString, ToSharedString, VecModel, Weak};
use ui::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut rx = Args::parse().build().await;

    let handle = tokio::runtime::Handle::current();
    let ui_thread = std::thread::spawn(move || {
        let ui = AppWindow::new().unwrap();

        let _ = ui.show();
        ui.window().set_size(LogicalSize::new(800.0, 600.0));
        ui.window().set_maximized(true);

        let (ui_tx, mut ui_rx) = tokio::sync::mpsc::unbounded_channel();

        ui.on_goto_parent({
            let ui_handle = ui.as_weak();
            let ui_tx = ui_tx.clone();

            move || {
                let Some(_ui) = ui_handle.upgrade() else {
                    return;
                };
                eprintln!("goto parent");
                let _ = ui_tx.send(UiEvent::GotoParent);
            }
        });

        ui.on_goto_path({
            let ui_handle = ui.as_weak();
            let ui_tx = ui_tx.clone();

            move |id| {
                let Some(_ui) = ui_handle.upgrade() else {
                    return;
                };
                eprintln!("goto path: {id}");
                let _ = ui_tx.send(UiEvent::GotoPath(id));
            }
        });

        // let rnk = ui.get_shr_rnk();
        // rnk.sort();

        let ui_handle = ui.as_weak();
        handle.spawn(async move {
            let mut path_tree = PathTree {
                begin: Some(std::time::Instant::now()),
                ..PathTree::default()
            };

            loop {
                tokio::select! {
                    event = rx.recv() => {
                        let Some(event) = event else {
                            break;
                        };

                        path_tree.event_cnt += 1;

                        path_tree.process_change(event);
                        path_tree.ui_change(&ui_handle, &rx);
                    }
                    Some(event) = ui_rx.recv() => {
                        path_tree.process_ui_event(&ui_handle, &rx, event);
                    }
                }
            }

            eprintln!("Finished processing");

            {
                let ui_handle = ui_handle.clone();
                slint::invoke_from_event_loop(move || {
                    let Some(ui) = ui_handle.upgrade() else {
                        return;
                    };

                    ui.set_disable_progress(true);
                })
                .report();
            }

            loop {
                let Some(event) = ui_rx.recv().await else {
                    break;
                };
                path_tree.process_ui_event(&ui_handle, &rx, event);
            }
        });

        let res = ui.run();
        if let Err(err) = res {
            eprintln!("Error running UI: {err}");
        }

        io::Result::Ok(())
    });

    let _ = tokio::task::spawn_blocking(|| ui_thread.join()).await?;
    Ok(())
}

#[derive(Default)]
struct PathTree {
    paths: HashMap<Option<PathId>, PathSlot>,
    total_files: usize,
    in_process_files: u64,
    process_events: u64,
    begin: Option<std::time::Instant>,
    event_cnt: u64,
    focus: Option<PathId>,
    focus_affected: bool,
}

impl PathTree {
    fn update_parent_size(&mut self, parent_id: Option<PathId>, size: u64, num_files: usize) {
        let parent = self.paths.entry(parent_id).or_default();
        let sz_cell = parent.size.get_or_insert_default();
        *sz_cell += size;
        parent.files += num_files;

        if !self.focus_affected && self.focus == parent_id {
            self.focus_affected = true;
        }
    }

    fn process_change(&mut self, event: EventRef) {
        let event = event.to_raw();
        match event {
            Event::Dir { path, parent } => {
                let parent_cell = self.paths.entry(parent).or_default();
                parent_cell.children.push(path);

                let child = self.paths.entry(Some(path)).or_default();
                child.parent = parent;

                self.in_process_files += 1;
                self.total_files += 1;
                self.process_events += 1;

                if !self.focus_affected && self.focus == Some(path) {
                    self.focus_affected = true;
                }
                if !self.focus_affected && self.focus == parent {
                    self.focus_affected = true;
                }
            }
            Event::FileFinish { path, parent, size } => {
                let parent_cell = self.paths.entry(parent).or_default();
                parent_cell.children.push(path);

                let child = self.paths.entry(Some(path)).or_default();
                child.size = Some(size);
                child.parent = parent;
                child.is_file = true;

                self.total_files += 1;
                self.process_events += 1;

                if !self.focus_affected && self.focus == Some(path) {
                    self.focus_affected = true;
                }
                if !self.focus_affected && self.focus == parent {
                    self.focus_affected = true;
                }

                self.update_parent_size(parent, size, 1);
            }
            Event::DirFinish {
                path,
                size,
                num_files,
            } => {
                let child = self.paths.entry(Some(path)).or_default();
                let parent = child.parent;
                self.update_parent_size(parent, size, num_files);

                let child = self.paths.entry(Some(path)).or_default();
                child.size = Some(size);
                child.files = num_files;

                self.in_process_files -= 1;
                self.process_events += 1;

                if !self.focus_affected && self.focus == Some(path) {
                    self.focus_affected = true;
                }
            }
        }
    }

    fn process_ui_event(&mut self, ui_handle: &Weak<AppWindow>, rx: &ShrRx, event: UiEvent) {
        match event {
            UiEvent::GotoParent => {
                let id = self
                    .focus
                    .and_then(|id| self.paths.get(&Some(id)).and_then(|node| node.parent));
                self.focus = id;
                self.focus_affected = true;
                self.ui_change(ui_handle, rx);
            }
            UiEvent::GotoPath(id) => {
                let id = id.parse::<NonZeroUsize>().ok().map(PathId::from_raw);
                self.focus = id;
                self.focus_affected = true;
                self.ui_change(ui_handle, rx);
            }
        }
    }

    fn ui_change(&mut self, ui_handle: &Weak<AppWindow>, rx: &ShrRx) {
        self.in_process_ui_change(ui_handle);
        self.focus_ui_change(ui_handle, rx);
    }

    fn in_process_ui_change(&mut self, ui_handle: &Weak<AppWindow>) {
        if self.process_events & 0xfff != 0 {
            return;
        }

        self.change_in_process_ui(ui_handle);
    }

    fn change_in_process_ui(&mut self, ui_handle: &Weak<AppWindow>) {
        let progress_files = self.in_process_files.to_shared_string();
        let total_files = self.total_files.to_shared_string();
        let elapsed = self.begin.map(|b| b.elapsed().as_secs_f32()).unwrap_or(1.);
        let speed = (self.process_events as f32) / elapsed;

        let ui_handle = ui_handle.clone();
        slint::invoke_from_event_loop(move || {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };

            ui.set_progress_files(progress_files);
            ui.set_total_files(total_files);
            ui.set_elapsed(elapsed);
            ui.set_speed(speed);
        })
        .report();
    }

    fn focus_ui_change(&mut self, ui_handle: &Weak<AppWindow>, rx: &ShrRx) {
        if !self.focus_affected {
            return;
        }
        self.focus_affected = false;

        let root_size = self
            .paths
            .get(&None)
            .and_then(|root| root.size)
            .filter(|s| *s > 0);

        let node = self.paths.get(&self.focus);

        let parent_size = node
            .and_then(|n| n.parent)
            .and_then(|n| (self.paths.get(&Some(n))))
            .and_then(|parent| parent.size);

        let mut v = Vec::default();

        let mut current = None;

        if let Some(node) = node {
            current = Some(Rank {
                path_id: self
                    .focus
                    .map(|id| id.into_raw().get())
                    .unwrap_or(0)
                    .to_shared_string(),
                path: self
                    .focus
                    .map(|focus| rx.get_path(focus).unwrap().to_str().unwrap().into())
                    .unwrap_or_default(),
                size: node
                    .size
                    .map(|s| human_readable_number(s, "si").to_shared_string())
                    .unwrap_or_default(),
                ratio: node.size.zip(parent_size).to_ratio(),
                all_ratio: node.size.zip(root_size).to_ratio(),
                is_file: node.is_file,
            });

            let parent_size = node.size;

            v = node
                .children
                .iter()
                .map(|p| {
                    let node = self.paths.get(&Some(*p));
                    let size = node.and_then(|n| n.size);
                    let ratio = size.zip(parent_size).to_ratio();
                    let all_ratio = size.zip(root_size).to_ratio();

                    Rank {
                        path_id: p.into_raw().get().to_shared_string(),
                        path: rx.get_path(*p).unwrap().to_str().unwrap().into(),
                        size: human_readable_number(size.unwrap_or(0), "si").to_shared_string(),
                        ratio,
                        all_ratio,
                        is_file: node.map(|n| n.is_file).unwrap_or(true),
                    }
                })
                .collect();

            v.sort_by(|a, b| {
                a.ratio
                    .partial_cmp(&b.ratio)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .reverse()
            });
        }

        let ui_handle = ui_handle.clone();
        slint::invoke_from_event_loop(move || {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };

            if let Some(current) = current {
                ui.set_current(current);
            }
            ui.set_ranks(Rc::new(VecModel::from(v)).into());
        })
        .report();
    }
}

#[derive(Debug, Default)]
struct PathSlot {
    parent: Option<PathId>,
    children: Vec<PathId>,
    size: Option<u64>,
    files: usize,
    is_file: bool,
}

enum UiEvent {
    GotoParent,
    GotoPath(SharedString),
}

trait ToRatio {
    fn to_ratio(self) -> f32;
}

impl ToRatio for Option<(u64, u64)> {
    fn to_ratio(self) -> f32 {
        self.map(|(s, p)| if p == 0 { 0. } else { s as f32 / p as f32 })
            .unwrap_or(0.)
    }
}
