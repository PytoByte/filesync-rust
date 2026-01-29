use bimap::BiHashMap;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct AppState {
    pub sync_checking: bool,
    pub syncing: bool,
    pub error_msg: String,
    pub system_path_input: String,
    pub server_path_input: String,
    pub host: String,
    pub login: String,
    pub password: String,
    pub pairs: BiHashMap<String, String>,
    pub pairs_syncstate: HashMap<String, SyncState>,
    pub editing: Option<EditingState>,
    pub authorization: bool
}

#[derive(Debug)]
pub enum EditingState {
    Create,
    Edit { key: String, value: String },
    Delete { key: String, value: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncState {
    Synchronized,
    UnsynchronizedServer,
    UnsynchronizedDevice,
    CantSynchronize
}

#[derive(Debug, Clone)]
pub enum Message {
    SystemPathInputChanged(String),
    ServerPathInputChanged(String),
    HostInputChanged(String),
    LoginInputChanged(String),
    PasswordInputChanged(String),
    CreatePair,
    EditPair(String),
    DeletePair(String),
    AcceptEditing,
    DeclineEditing,
    CloseError,
    SynchronizeCheck,
    StopSynchronizeCheck,
    Synchronize,
    StopSynchronize,
    UpdatePairSyncState(String, SyncState),
    OpenAuth,
    CloseAuth,
    ShowError(String)
}
