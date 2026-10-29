//! [`Remote`] for a real Overleaf project: the zip download for reading, the
//! upload endpoint for writing, and the project tree for finding what to
//! delete.

use crate::engine::Remote;
use crate::files::{from_zip, Loaded};
use canopy_overleaf::{Client, Folder};
use std::sync::Mutex;

/// One project on one server.
pub struct OverleafRemote {
    client: Client,
    project_id: String,
    // read with each fetch; uploads need the root folder's id, deletes need
    // entity ids by path
    tree: Mutex<Option<Folder>>,
}

impl OverleafRemote {
    /// A remote for `project_id`, talking through `client`.
    pub fn new(client: Client, project_id: impl Into<String>) -> Self {
        Self {
            client,
            project_id: project_id.into(),
            tree: Mutex::new(None),
        }
    }

    fn root_id(&self) -> Result<String, String> {
        self.tree
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|t| t.id.clone())
            .ok_or_else(|| "the project tree isn't loaded".to_string())
    }
}

impl Remote for OverleafRemote {
    async fn fetch(&self) -> Result<Loaded, String> {
        let tree = self
            .client
            .tree(&self.project_id)
            .await
            .map_err(|e| e.to_string())?;
        *self.tree.lock().unwrap_or_else(|e| e.into_inner()) = Some(tree);
        let zip = self
            .client
            .download_zip(&self.project_id)
            .await
            .map_err(|e| e.to_string())?;
        from_zip(&zip).map_err(|e| e.to_string())
    }

    async fn upload(&self, path: &str, data: Vec<u8>) -> Result<(), String> {
        let root = self.root_id()?;
        self.client
            .upload(&self.project_id, &root, path, data)
            .await
            .map_err(|e| format!("{path}: {e}"))
    }

    async fn delete(&self, path: &str) -> Result<(), String> {
        let found = self
            .tree
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|t| t.find(path));
        match found {
            Some(entry) => self
                .client
                .delete(&self.project_id, entry.kind, &entry.id)
                .await
                .map_err(|e| format!("{path}: {e}")),
            // already gone on the server
            None => Ok(()),
        }
    }
}
