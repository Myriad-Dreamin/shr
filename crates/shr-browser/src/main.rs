//! Browser impl

// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
            let mut path_tree = PathTree::default();

            let mut event_cnt = 0u64;
            let mut now = std::time::Instant::now();

            loop {
                tokio::select! {
                    event = rx.recv() => {
                        let Some(event) = event else {
                            break;
                        };

                        event_cnt += 1;

                        path_tree.process_change(event);
                        path_tree.ui_change(&ui_handle, &rx);

                        if event_cnt % 10000 == 0 {
                            let elapsed = now.elapsed();
                            eprintln!("Processed {event_cnt} events in {elapsed:?}");
                            now = std::time::Instant::now();
                        }

                    }
                    Some(event) = ui_rx.recv() => {
                        path_tree.process_ui_event(&ui_handle, &rx, event);
                    }
                }
            }

            let elapsed = now.elapsed();
            eprintln!("Finished processing {event_cnt} events in {elapsed:?}");

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
    focus: Option<PathId>,
    focus_affected: bool,
}

impl PathTree {
    fn update_parent_size(&mut self, parent_id: Option<PathId>, size: u64, num_files: usize) {
        if !self.focus_affected && self.focus.is_none() {
            self.focus_affected = true;
        }

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

                if !self.focus_affected && self.focus == Some(path) {
                    self.focus_affected = true;
                }
            }
            Event::FileFinish { path, parent, size } => {
                let parent_cell = self.paths.entry(parent).or_default();
                parent_cell.children.push(path);

                let child = self.paths.entry(Some(path)).or_default();
                child.size = Some(size);
                child.parent = parent;

                if !self.focus_affected && self.focus == Some(path) {
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
        if !self.focus_affected {
            return;
        }
        self.focus_affected = false;

        let root_size = self
            .paths
            .get(&None)
            .and_then(|root| root.size)
            .unwrap_or(0)
            .max(1);

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
                ratio: node
                    .size
                    .zip(parent_size)
                    .map(|(s, p)| if p == 0 { 0. } else { s as f32 / p as f32 })
                    .unwrap_or(0.),
                all_ratio: node.size.map(|s| s as f32 / root_size as f32).unwrap_or(0.),
            });

            let parent_size = node.size;

            v = node
                .children
                .iter()
                .map(|p| {
                    let node = self.paths.get(&Some(*p));
                    let size = node.and_then(|n| n.size).unwrap_or(0);
                    let ratio = parent_size
                        .map(|p| if p == 0 { 0. } else { size as f32 / p as f32 })
                        .unwrap_or(0.);
                    let all_ratio = size as f32 / root_size as f32;

                    Rank {
                        path_id: p.into_raw().get().to_shared_string(),
                        path: rx.get_path(*p).unwrap().to_str().unwrap().into(),
                        size: human_readable_number(size, "si").to_shared_string(),
                        ratio,
                        all_ratio,
                    }
                })
                .collect();

            v.sort_by(|a, b| {
                a.all_ratio
                    .partial_cmp(&b.all_ratio)
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
}

enum UiEvent {
    GotoParent,
    GotoPath(SharedString),
}

// use std::{
//     fmt::Display,
//     io::{self, Write},
//     sync::{Arc, Mutex},
// };

// use args::Args;
// use clap::Parser;
// use futures::Stream;
// use shr::{EventRef, ShrRx};
// use std::convert::Infallible;
// use std::net::SocketAddr;

// use http_body_util::{Channel, Full, StreamBody, combinators::BoxBody};
// use hyper::{Request, Response, body::Frame};
// use hyper::{StatusCode, service::service_fn};
// use hyper::{body::Bytes, header::CONTENT_TYPE};
// use hyper::{header::CACHE_CONTROL, server::conn::http1};
// use hyper_util::rt::TokioIo;
// use tokio::net::{TcpListener, TcpStream};

// mod args;

// #[tokio::main]
// async fn main() -> anyhow::Result<()> {
//     let server = tiny_http::Server::http("127.0.0.1:0")
//         .map_err(|e| anyhow::anyhow!("cannot start http: {e}"))?;

//     let local_addr = server.server_addr().to_ip().unwrap();
//     let addr = format!("http://{local_addr}");

//     if let Err(err) = open::that(addr) {
//         eprintln!("cannot open browser: {err}");
//     }

//     tokio::spawn(async move {
//         let _ = tokio::signal::ctrl_c().await;
//         eprintln!("Ctrl-C received, exiting");
//         std::process::exit(0);
//     });

//     // We create a TcpListener and bind it to 127.0.0.1:3000
//     let listener = TcpListener::bind("127.0.0.1:0").await?;

//     // We start a loop to continuously accept incoming connections
//     loop {
//         let (stream, _) = listener.accept().await?;

//         // Use an adapter to access something implementing `tokio::io` traits as if they implement
//         // `hyper::rt` IO traits.
//         let io = TokioIo::new(stream);

//         // Spawn a tokio task to serve multiple connections concurrently
//         tokio::task::spawn(async move {
//             // Finally, we bind the incoming connection to our `hello` service
//             if let Err(err) = http1::Builder::new()
//                 // `service_fn` converts our function in a `Service`
//                 .serve_connection(io, service_fn(hello))
//                 .await
//             {
//                 eprintln!("Error serving connection: {:?}", err);
//             }
//         });
//     }
// }

// struct Host {}

// impl Host {
//     async fn serve(&mut self, mut rx: ShrRx, listener: TcpListener) {
//         loop {
//             tokio::select! {
//                 accepted = listener.accept() => {
//                     self.new_request(accepted.unwrap().0);
//                 }
//                 Some(event) = rx.recv() => {
//                     self.process_change(event);
//                 }
//             }
//         }
//     }

//     fn new_request(&mut self, stream: TcpStream) {

//     }

//     fn process_change(&mut self, event: EventRef) {
//         let event: Arc<str> = serde_json::to_string(&event.to_raw())
//             .report()
//             .unwrap()
//             .into();
//         // let _ = stx.send(event).report();
//     }
// }

// async fn hello(_: Request<hyper::body::Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
//     Ok(Response::new(Full::new(Bytes::from("Hello, World!"))))
// }

// /// The event yield by `shr-browser`.
// #[derive(Debug)]
// #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
// #[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "camelCase"))]
// enum ServerEvent {
//     /// A file entry is found.
//     Id {
//         /// The id.
//         id: usize,
//     },
// }

// #[derive(Clone, Default)]
// struct Inner {}

// #[derive(Clone, Default)]
// struct EventChannel(Arc<Mutex<Inner>>);

// struct EventStream {
//     inner: EventChannel,
// }

// impl EventStream {
//     pub async fn recv(&mut self) -> Option<Arc<str>> {
//         todo!()
//     }
// }

// /// Event stream for the `/events` route.
// async fn handle_events_blocking(
//     req: Request<hyper::body::Incoming>,
//     stream: Channel<Arc<str>>,
// ) -> Result<Response<Channel<Arc<str>>>, Infallible> {
//     Ok(Response::builder()
//         .status(StatusCode::OK)
//         .header(CONTENT_TYPE, "text/event-stream")
//         .header(CACHE_CONTROL, "no-cache")
//         .body(stream)
//         .expect("Failed to build Response"))
// }

// trait Report {
//     type Target;

//     fn report(self) -> Option<Self::Target>
//     where
//         Self: Sized;
// }

// impl<T, E: Display> Report for Result<T, E> {
//     type Target = T;

//     fn report(self) -> Option<T> {
//         match self {
//             Ok(v) => Some(v),
//             Err(e) => {
//                 eprintln!("failed io: {e}");
//                 None
//             }
//         }
//     }
// }
