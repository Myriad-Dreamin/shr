// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use compio::runtime::RuntimeBuilder;
use futures::channel::mpsc::SendError;
use futures::channel::{mpsc, oneshot};
use futures::future::LocalBoxFuture;
use futures::{SinkExt, StreamExt};

use super::ShrTask;

/// This is arbitrary, but since all tasks are spawned instantly, we shouldn't need a too big buffer.
const CHANNEL_SIZE: usize = 4;

fn task<F, Fut, T>(func: F) -> (Task<T>, SpawnTask)
where
    F: (FnOnce() -> Fut) + Send + 'static,
    Fut: Future<Output = T>,
    T: Send + 'static,
{
    let (tx, recv) = oneshot::channel();

    let boxed = Box::new(|| {
        Box::pin(async move {
            let res = func().await;
            tx.send(res).ok();
        }) as _
    });

    (Task(recv), SpawnTask(boxed))
}

/// A task handle that can be used to retrieve result spawned into [`CompioThread`].
pub struct Task<T>(oneshot::Receiver<T>);

impl<T> Task<T> {
    pub async fn wait(self) -> Result<T, oneshot::Canceled> {
        self.0.await
    }
}

/// Type erased task that can be spawned into a [`CompioThread`].
struct SpawnTask(Box<dyn (FnOnce() -> LocalBoxFuture<'static, ()>) + Send>);

impl SpawnTask {
    fn call(self) -> LocalBoxFuture<'static, ()> {
        (self.0)()
    }
}

#[derive(Debug)]
pub struct CompioThread {
    handle: SpawnHandle,
}

impl CompioThread {
    pub fn new(builder: RuntimeBuilder) -> Self {
        let (send, mut recv) = mpsc::channel(CHANNEL_SIZE);
        let handle = SpawnHandle(send);
        let _thread = std::thread::spawn(move || {
            let rt = builder.build().expect("failed to create runtime");
            rt.block_on(async {
                // let dispatcher = compio::dispatcher::Dispatcher::new().unwrap();
                while let Some(task) = recv.next().await {
                    rt.spawn(task.call()).detach();

                    // let _ = dispatcher.dispatch(|| task.call()).unwrap();
                }
            });
        });
        Self { handle }
    }

    pub async fn spawn(&self, func: ShrTask) -> Result<Task<Option<(usize, u64)>>, SendError> {
        self.handle.clone().spawn(func).await
    }

    pub async fn spawn_read_dir(
        &self,
        func: ShrTask,
    ) -> Result<Task<Option<(usize, u64)>>, SendError> {
        self.handle.clone().spawn_read_dir(func).await
    }
}

#[derive(Debug, Clone)]
pub struct SpawnHandle(mpsc::Sender<SpawnTask>);

impl SpawnHandle {
    pub async fn spawn(&mut self, func: ShrTask) -> Result<Task<Option<(usize, u64)>>, SendError> {
        let (task, spawn) = task(|| func.exec());
        self.0.send(spawn).await?;
        Ok(task)
    }

    pub async fn spawn_read_dir(
        &mut self,
        func: ShrTask,
    ) -> Result<Task<Option<(usize, u64)>>, SendError> {
        let (task, spawn) = task(|| func.scan_dir());
        self.0.send(spawn).await?;
        Ok(task)
    }
}
