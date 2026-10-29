//! Two-way sync between an Overleaf project and a folder on disk.
//!
//! A sync compares three versions of every file: the last synced one (the
//! *base*, kept in a hidden git repository, see [`store`]), the one in the
//! folder, and the one on the server. [`plan`] decides from those alone what
//! to download, upload, merge or flag, without touching disk or network, and
//! [`engine`] carries the plan out.
//!
//! Every path that comes from the server is untrusted and goes through
//! [`project_path`] before it touches disk.

pub mod engine;
pub mod files;
pub mod ignore;
pub mod overleaf;
mod paths;
pub mod plan;
pub mod store;

pub use engine::{restore, sync, write_project_file, Options, Remote, Report, Restored, SyncError};
pub use files::{Files, Loaded, Skipped};
pub use overleaf::OverleafRemote;
pub use paths::{project_path, resolve, PathError};
pub use plan::{plan, Conflict, ConflictKind, Plan, Step};
pub use store::{Change, ChangeKind, Snapshot, Store};

use serde::{Deserialize, Serialize};

/// Where a synced project stands, shown in the sidebar and status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncState {
    /// Not linked to a local folder.
    #[default]
    Off,
    /// Local and remote match.
    Synced,
    /// A pull or push is running.
    Syncing,
    /// Local edits not pushed yet (offline, or waiting to batch).
    Pending,
    /// Both sides changed the same lines; the user has to choose.
    Conflict,
    /// The last attempt failed (network, sign-in expired).
    Error,
}
