use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct AppState {
    pub system_path_input: String,
    pub server_path_input: String,
    pub pairs: HashMap<String, String>,
    pub editing: Option<EditingState>
}

#[derive(Debug)]
pub enum EditingState {
    Create,
    Edit {key: String, value: String},
    Delete {key: String, value: String}
}

#[derive(Debug, Clone)]
pub enum Message {
    SystemPathInputChanged(String),
    ServerPathInputChanged(String),
    CreatePair,
    EditPair(String),
    DeletePair(String),
    AcceptEditing,
    DeclineEditing
}