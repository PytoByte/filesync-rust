#![windows_subsystem = "windows"]

mod webdav;
mod db;

use std::{collections::HashMap, path::Path};

use iced::{
    Element, Fill, Subscription, Task, stream,
    widget::{button, column, row, rule, scrollable, text, text_input}
};
use tokio::runtime::Runtime;
use typed_path::UnixPath;
use bimap::BiHashMap;

use crate::db::{AUTH_TABLE, PAIRS_TABLE};

fn main() -> iced::Result {
    iced::application(AppState::new, AppState::update, AppState::view)
    .title("filesync")
    .subscription(AppState::subscription)
    .run()
}

fn is_valid_unix_path(path: &str) -> bool {
    UnixPath::new(path).is_valid()
}

#[derive(Debug, Default)]
pub struct AppState {
    // Flags
    pub syncing: bool,
    pub sync_checking: bool,
    pub authorization: bool,
    // Text inputs
    pub host: String,
    pub login: String,
    pub password: String,
    pub local_path_input: String,
    pub remote_path_input: String,
    // Synchronization pairs
    pub pairs: BiHashMap<String, String>,
    pub pairs_syncstate: HashMap<String, SyncState>,
    pub editing: Option<EditingState>,
    // Error messages
    pub error_msg: String,
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
    // Text inputs
    HostInputChanged(String),
    LoginInputChanged(String),
    PasswordInputChanged(String),
    SystemPathInputChanged(String),
    ServerPathInputChanged(String),
    // Editing
    CreatePair,
    EditPair(String),
    DeletePair(String),
    AcceptEditing,
    DeclineEditing,
    // Synchronization events
    Synchronize,
    SynchronizeCheck,
    StopSynchronize,
    StopSynchronizeCheck,
    UpdatePairSyncState(String, SyncState),
    // Auth
    OpenAuth,
    CloseAuth,
    // Error messages
    ShowError(String),
    CloseError
}

impl AppState {
    fn new() -> AppState {
        let pairs_table = db::read_as_hashmap(PAIRS_TABLE).unwrap_or_default();
        let auth_table = db::read_as_hashmap(AUTH_TABLE).unwrap_or_default();

        AppState {
            // Flags
            syncing: false,
            sync_checking: true,
            authorization: false,
            // Text inputs
            host: auth_table.get_by_left("host").unwrap_or(&"".to_string()).to_owned(),
            login: auth_table.get_by_left("login").unwrap_or(&"".to_string()).to_owned(),
            password: auth_table.get_by_left("password").unwrap_or(&"".to_string()).to_owned(),
            local_path_input: String::new(),
            remote_path_input: String::new(),
            // Synchronization pairs
            pairs: pairs_table,
            pairs_syncstate: HashMap::new(),
            editing: None,
            // Error messages
            error_msg: String::new(),
        }
    }

    fn update(self: &mut Self, message: Message) -> Task<Message> {
        match message {
            Message::HostInputChanged(host) => {
                self.host = host;
                Task::none()
            }
            Message::LoginInputChanged(login) => {
                self.login = login;
                Task::none()
            }
            Message::PasswordInputChanged(password) => {
                self.password = password;
                Task::none()
            }
            Message::SystemPathInputChanged(input) => {
                self.local_path_input = input;
                Task::none()
            }
            Message::ServerPathInputChanged(input) => {
                self.remote_path_input = input;
                Task::none()
            }
            Message::CreatePair => {
                if self.editing.is_some() {
                    self.decline_editing();
                    self.clear_editing();
                }

                self.editing = Some(EditingState::Create);
                Task::none()
            }
            Message::EditPair(key) => {
                if self.editing.is_some() {
                    self.decline_editing();
                    self.clear_editing();
                }

                if let Some((key, value)) = self.pairs.remove_by_left(&key) {
                    self.local_path_input = key.clone();
                    self.remote_path_input = value.clone();
                    self.editing = Some(EditingState::Edit {
                        key: key,
                        value: value,
                    });
                }
                Task::none()
            }
            Message::DeletePair(key) => {
                if self.editing.is_some() {
                    self.decline_editing();
                    self.clear_editing();
                }

                if let Some((key, value)) = self.pairs.remove_by_left(&key) {
                    self.editing = Some(EditingState::Delete {
                        key: key,
                        value: value,
                    });
                }
                Task::none()
            }
            Message::AcceptEditing => {
                match &self.editing {
                    Some(EditingState::Create | EditingState::Edit { .. }) => {
                        if self.local_path_input.is_empty() || self.remote_path_input.is_empty() {
                            self.error_msg = String::from("Empty path");
                            return Task::none();
                        }

                        if !Path::new(&self.local_path_input).exists() {
                            self.error_msg = String::from("System path not found");
                            return Task::none();
                        }

                        if !is_valid_unix_path(&self.remote_path_input) {
                            self.error_msg = String::from("Server path is invalid");
                            return Task::none();
                        }

                        if self.pairs.contains_left(&self.local_path_input) {
                            self.error_msg = String::from("This system path already in use");
                            return Task::none();
                        }

                        if self.pairs.contains_right(&self.remote_path_input) {
                            self.error_msg = String::from("This server path already in use");
                            return Task::none();
                        }
                        
                        self.local_path_input = typed_path::NativePath::new(&self.local_path_input).absolutize().unwrap().to_string();
                        self.remote_path_input = UnixPath::new(&format!("/{}", self.remote_path_input)).absolutize().unwrap().to_string();

                        match db::write(PAIRS_TABLE, &self.local_path_input, &self.remote_path_input) {
                            Ok(_) => {
                                self.pairs.insert(
                                    self.local_path_input.clone(),
                                    self.remote_path_input.clone(),
                                );
                                self.clear_editing();
                            }
                            Err(e) => {
                                self.error_msg = e.to_string();
                            }
                        }
                    }
                    Some(EditingState::Delete { key, .. }) => {
                        match db::delete(PAIRS_TABLE, &key) {
                            Ok(_) => {
                                self.clear_editing();
                            }
                            Err(e) => {
                                self.error_msg = e.to_string();
                            }
                        }
                    }
                    _ => {}
                }

                Task::none()
            }
            Message::DeclineEditing => {
                self.decline_editing();
                Task::none()
            }
            Message::Synchronize => {
                self.syncing = true;
                Task::none()
            }
            Message::SynchronizeCheck => {
                self.sync_checking = true;
                Task::none()
            }
            Message::StopSynchronize => {
                self.syncing = false;
                Task::none()
            }
            Message::StopSynchronizeCheck => {
                self.sync_checking = false;
                Task::none()
            }
            Message::UpdatePairSyncState(key, syncstate) => {
                self.pairs_syncstate.insert(key, syncstate);
                Task::none()
            }
            Message::OpenAuth => {
                self.decline_editing();
                self.authorization = true;
                Task::none()
            }
            Message::CloseAuth => {
                self.authorization = false;
                db::write(AUTH_TABLE, "host", &self.host).unwrap();
                db::write(AUTH_TABLE, "login", &self.login).unwrap();
                db::write(AUTH_TABLE, "password", &self.password).unwrap();
                Task::none()
            }
            Message::ShowError(error_msg) => {
                self.error_msg = error_msg;
                Task::none()
            }
            Message::CloseError => {
                self.error_msg.clear();
                Task::none()
            }
        }
    }

    fn decline_editing(self: &mut Self) {
        if let Some(editing) = &self.editing {
            match editing {
                EditingState::Create => {}
                EditingState::Edit { key, value } | EditingState::Delete { key, value } => {
                    self.pairs.insert(key.clone(), value.clone());
                }
            }
        }
        self.clear_editing();
    }

    fn clear_editing(self: &mut Self) {
        self.local_path_input.clear();
        self.remote_path_input.clear();
        self.editing = None;
    }

    fn input_editing_fields(self: &'_ Self) -> Element<'_, Message> {
        row![
            text_input("System path", &self.local_path_input)
                .on_input(Message::SystemPathInputChanged),
            text("<=>"),
            text_input("Server path", &self.remote_path_input)
                .on_input(Message::ServerPathInputChanged)
        ].spacing(8).into()
    }

    fn editing_buttons(self: &'_ Self) -> Element<'_, Message> {
        row![
            button(text("Accept")).on_press(Message::AcceptEditing),
            button(text("Decline")).on_press(Message::DeclineEditing)
        ].spacing(8).into()
    }

    fn view(self: &'_ Self) -> Element<'_, Message> {
        let mut content = column!().spacing(8).padding(8);

        if let Some(editing) = &self.editing {
            match editing {
                EditingState::Create => {
                    content = content.push(column![
                        text("Creating pair"),
                        self.input_editing_fields(),
                        self.editing_buttons()
                    ].spacing(3));
                },
                EditingState::Edit { key: _, value: _ } => {
                    content = content.push(column![
                        text("Editing pair"),
                        self.input_editing_fields(),
                        self.editing_buttons()
                    ].spacing(3));
                },
                EditingState::Delete { key, value } => {
                    content = content.push(column![
                        text("Are you sure to delete this pair?"),
                        text(format!("{key} <=> {value}")),
                        self.editing_buttons()
                    ].spacing(3))
                }
            }

            content = content.push(rule::horizontal(3));
        }

        if self.authorization {
            content = content.push(
                column![
                    text("Authorization"),
                    text_input("Host", &self.host).width(Fill).on_input(Message::HostInputChanged),
                    text_input("Login", &self.login).width(Fill).on_input(Message::LoginInputChanged),
                    text_input("Password", &self.password).width(Fill).on_input(Message::PasswordInputChanged),
                    button(text("Save")).on_press(Message::CloseAuth),
                ].spacing(3),
            );
            content = content.push(rule::horizontal(3));
        }

        if !self.error_msg.is_empty() {
            content = content.push(
                column![
                    text(format!("Error: {}", self.error_msg)),
                    button(text("Close")).on_press(Message::CloseError)
                ]
                .spacing(3),
            );
            content = content.push(rule::horizontal(3));
        }

        content = content.push(
            button(text("New pair").center().width(Fill))
                .width(Fill)
                .on_press(Message::CreatePair),
        );

        let mut pairs_content = column!().spacing(2);

        for (key, value) in self.pairs.iter() {
            let syncstate_description = match self.pairs_syncstate.get(key) {
                Some(SyncState::Synchronized) => {
                    "✅"
                },
                Some(SyncState::UnsynchronizedDevice) => {
                    "☁️➡️💻"
                },
                Some(SyncState::UnsynchronizedServer) => {
                    "💻➡️☁️"
                },
                Some(SyncState::CantSynchronize) => {
                    "❌"
                },
                None => {
                    "❓"
                }
            };

            pairs_content = pairs_content.push(
                row![
                    text(format!("({syncstate_description}) {key} <=> {value}")).width(Fill),
                    button(text("Edit")).on_press(Message::EditPair(key.clone())),
                    button(text("Delete")).on_press(Message::DeletePair(key.clone()))
                ]
                .spacing(8),
            );
        }

        content = content.push(scrollable(pairs_content).height(Fill));

        if !self.authorization {
            if !self.sync_checking {
                if self.syncing {
                    content = content.push(button(text("Stop synchronize").center().width(Fill)).width(Fill).on_press(Message::StopSynchronize));
                } else {
                    content = content.push(button(text("Synchronize").center().width(Fill)).width(Fill).on_press(Message::Synchronize));
                }
            }

            if !self.syncing {
                if self.sync_checking {
                    content = content.push(button(text("Stop checking").center().width(Fill)).width(Fill).on_press(Message::StopSynchronizeCheck));
                } else {
                    content = content.push(button(text("Check").center().width(Fill)).width(Fill).on_press(Message::SynchronizeCheck));
                }
            }

            if !self.syncing && !self.sync_checking {
                content = content.push(button(text("Authorization").center().width(Fill)).width(Fill).on_press(Message::OpenAuth));
            }
        }

        content.into()
    }

    fn subscription(self: &Self) -> Subscription<Message> {
        if self.syncing {
            let pairs_vec: Vec<(String, String)> = self.pairs.iter().map(|(k, v)| {(k.clone(), v.clone())}).collect();

            Subscription::run_with((self.host.clone(), self.login.clone(), self.password.clone(), pairs_vec), |(host, login, password, pairs_vec)| {
                let pairs_vec = pairs_vec.clone();
                let host = host.clone();
                let login = login.clone();
                let password = password.clone();
                stream::channel(100, |output| async move {
                    let rt = Runtime::new().unwrap();
                    rt.block_on(async {
                        webdav::run_sync(output, host, login, password, pairs_vec).await;
                    });
                })
            })
        } else if self.sync_checking {
            let pairs_vec: Vec<(String, String)> = self.pairs.iter().map(|(k, v)| {(k.clone(), v.clone())}).collect();

            Subscription::run_with((self.host.to_owned(), self.login.to_owned(), self.password.to_owned(), pairs_vec), |(host, login, password, pairs_vec)| {
                let pairs_vec = pairs_vec.clone();
                let host = host.clone();
                let login = login.clone();
                let password = password.clone();
                stream::channel(100, |output| async move {
                    let rt = Runtime::new().unwrap();
                    rt.block_on(async {
                        webdav::check_sync(output, host, login, password, pairs_vec).await;
                    });
                })
            })
        } else {
            Subscription::none()
        }
    }
}