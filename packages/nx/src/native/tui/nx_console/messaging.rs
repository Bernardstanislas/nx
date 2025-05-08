use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use tracing::trace;

use jsonrpsee::{
    async_client::{Client, ClientBuilder},
    proc_macros::rpc,
};

use crate::native::{
    tui::{
        components::tasks_list::{TaskItem, TaskStatus},
        nx_console::ipc_transport::IpcTransport,
        pty::PtyInstance,
    },
    utils::socket_path::get_full_nx_console_socket_path,
};

#[derive(Serialize, Deserialize)]
pub struct UpdatedRunningTask {
    pub name: String,
    pub status: TaskStatus,
    pub output: String,
}

#[rpc(client, namespace = "nx", namespace_separator = "/")]
pub trait ConsoleRpc {
    #[method(name = "terminalMessage")]
    fn terminal_message(&self, text: String);

    #[method(name = "updateRunningTasks")]
    fn update_running_tasks(&self, updates: Vec<UpdatedRunningTask>);
    #[method(name = "startedRunningTasks")]
    fn start_running_tasks(&self, process_id: i32);
    #[method(name = "endedRunningTasks")]
    fn end_running_tasks(&self, process_id: i32);
}

pub struct NxConsoleMessageConnection {
    client: Option<Arc<Client>>,
}

impl NxConsoleMessageConnection {
    pub async fn new(workspace_root: &str) -> Self {
        let socket_path = get_full_nx_console_socket_path(workspace_root);
        let client = IpcTransport::new(socket_path)
            .await
            .map(|transport| {
                ClientBuilder::new().build_with_tokio(transport.writer, transport.reader)
            })
            .inspect_err(|e| {
                trace!("Could not connect to Nx Console: {}", e);
            })
            .ok()
            .map(Arc::new);

        Self { client }
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    pub fn send_terminal_string(&self, message: impl Into<String>) -> Option<()> {
        self.client.as_ref().map(|client| {
            let message = message.into();
            let client = client.clone();
            tokio::spawn(async move {
                if let Err(e) = client.terminal_message(message).await {
                    trace!("Failed to send terminal message: {}", e);
                }
            });
        })
    }

    pub fn update_running_tasks(
        &self,
        task_statuses: &[TaskItem],
        ptys: &HashMap<String, Arc<PtyInstance>>,
    ) -> Option<()> {
        self.client.as_ref().map(|client| {
            let client = client.clone();

            let task_statuses: Vec<UpdatedRunningTask> = task_statuses
                .iter()
                .map(|task| {
                    let output = ptys
                        .get(&task.name)
                        .and_then(|pty| pty.get_screen())
                        .map(|screen| screen.all_contents())
                        .unwrap_or_default();
                    UpdatedRunningTask {
                        name: task.name.clone(),
                        status: task.status,
                        output,
                    }
                })
                .collect();

            tokio::spawn(async move {
                if let Err(e) = client.update_running_tasks(task_statuses).await {
                    trace!("Failed to send task statuses: {}", e);
                }
            });
        })
    }

    pub fn start_running_tasks(&self) -> Option<()> {
        self.client.as_ref().map(|client| {
            let client = client.clone();
            let process_id = std::process::id() as i32;
            tokio::spawn(async move {
                if let Err(e) = client.start_running_tasks(process_id).await {
                    trace!("Failed to send start running tasks: {}", e);
                }
            });
        })
    }

    pub fn end_running_tasks(&self) -> Option<()> {
        self.client.as_ref().map(|client| {
            let client = client.clone();
            let process_id = std::process::id() as i32;
            tokio::spawn(async move {
                if let Err(e) = client.end_running_tasks(process_id).await {
                    trace!("Failed to send end running tasks: {}", e);
                }
            });
        })
    }
}
