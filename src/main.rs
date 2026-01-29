use std::{collections::HashMap, path::Path};

use iced::{
    Element, Fill, Subscription, Task, stream, widget::{button, column, row, rule, scrollable, text, text_input}
};

use tokio::runtime::Runtime;

mod webdav;
mod state;
mod views;
mod db;

use state::{AppState, EditingState, Message};

use crate::{db::{AUTH_TABLE, PAIRS_TABLE}, state::SyncState, views::popup};

fn new() -> AppState {
    let pairs_table = db::read_as_hashmap(PAIRS_TABLE).unwrap_or_default();
    let auth_table = db::read_as_hashmap(AUTH_TABLE).unwrap_or_default();

    AppState {
        authorization: false,
        sync_checking: true,
        syncing: false,
        error_msg: String::new(),
        system_path_input: String::new(),
        server_path_input: String::new(),
        host: auth_table.get_by_left("host").unwrap_or(&"".to_string()).to_owned(),
        login: auth_table.get_by_left("login").unwrap_or(&"".to_string()).to_owned(),
        password: auth_table.get_by_left("password").unwrap_or(&"".to_string()).to_owned(),
        pairs: pairs_table,
        pairs_syncstate: HashMap::new(),
        editing: None,
    }
}

fn decline_editing(state: &mut AppState) {
    if let Some(editing) = &state.editing {
        match editing {
            EditingState::Create => {}
            EditingState::Edit { key, value } | EditingState::Delete { key, value } => {
                state.pairs.insert(key.clone(), value.clone());
            }
        }
    }
}

fn clear_editing(state: &mut AppState) {
    state.system_path_input.clear();
    state.server_path_input.clear();
    state.editing = None;
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::SystemPathInputChanged(input) => {
            state.system_path_input = input;
            Task::none()
        }
        Message::ServerPathInputChanged(input) => {
            state.server_path_input = input;
            Task::none()
        }
        Message::CreatePair => {
            if state.editing.is_some() {
                decline_editing(state);
                clear_editing(state);
            }

            state.editing = Some(EditingState::Create);
            Task::none()
        }
        Message::EditPair(key) => {
            if state.editing.is_some() {
                decline_editing(state);
                clear_editing(state);
            }

            if let Some((key, value)) = state.pairs.remove_by_left(&key) {
                state.system_path_input = key.clone();
                state.server_path_input = value.clone();
                state.editing = Some(EditingState::Edit {
                    key: key,
                    value: value,
                });
            }
            Task::none()
        }
        Message::DeletePair(key) => {
            if state.editing.is_some() {
                decline_editing(state);
                clear_editing(state);
            }

            if let Some((key, value)) = state.pairs.remove_by_left(&key) {
                state.editing = Some(EditingState::Delete {
                    key: key,
                    value: value,
                });
            }
            Task::none()
        }
        Message::AcceptEditing => {
            if let Some(EditingState::Create | EditingState::Edit { .. }) = &state.editing {
                if state.system_path_input.is_empty() && state.server_path_input.is_empty() {
                    clear_editing(state);
                    return Task::none();
                }

                if !Path::new(&state.system_path_input).exists() {
                    state.error_msg = String::from("System path not found");
                    decline_editing(state);
                    clear_editing(state);
                    return Task::none();
                }

                match db::write(PAIRS_TABLE, &state.system_path_input, &state.server_path_input) {
                    Ok(_) => {
                        state.pairs.insert(
                            state.system_path_input.clone(),
                            state.server_path_input.clone(),
                        );
                    }
                    Err(e) => {
                        decline_editing(state);
                        state.error_msg = e.to_string();
                    }
                }
            } else if let Some(EditingState::Delete { key, .. }) = &state.editing {
                if let Err(e) = db::delete(PAIRS_TABLE, &key) {
                    decline_editing(state);
                    state.error_msg = e.to_string();
                }
            }
            clear_editing(state);
            Task::none()
        }
        Message::DeclineEditing => {
            decline_editing(state);
            clear_editing(state);
            Task::none()
        }
        Message::CloseError => {
            state.error_msg.clear();
            Task::none()
        },
        Message::Synchronize => {
            state.syncing = true;
            Task::none()
        },
        Message::StopSynchronize => {
            state.syncing = false;
            Task::none()
        },
        Message::UpdatePairSyncState(key, syncstate) => {
            dbg!(&syncstate);
            state.pairs_syncstate.insert(key, syncstate);
            Task::none()
        },
        Message::SynchronizeCheck => {
            state.sync_checking = true;
            Task::none()
        },
        Message::StopSynchronizeCheck => {
            state.sync_checking = false;
            Task::none()
        },
        Message::OpenAuth => {
            state.authorization = true;
            Task::none()
        },
        Message::CloseAuth => {
            state.authorization = false;
            db::write(AUTH_TABLE, "host", &state.host).unwrap();
            db::write(AUTH_TABLE, "login", &state.login).unwrap();
            db::write(AUTH_TABLE, "password", &state.password).unwrap();
            Task::none()
        },
        Message::HostInputChanged(host) => {
            state.host = host;
            Task::none()
        },
        Message::LoginInputChanged(login) => {
            state.login = login;
            Task::none()
        },
        Message::PasswordInputChanged(password) => {
            state.password = password;
            Task::none()
        },
        Message::ShowError(error_msg) => {
            state.error_msg = error_msg;
            Task::none()
        }
    }
}

fn view(state: &'_ AppState) -> Element<'_, Message> {
    let mut content = column!().spacing(8).padding(8);

    if let Some(editing) = &state.editing {
        match editing {
            EditingState::Create => {
                content = content.push(popup::create(state));
            },
            EditingState::Edit { key: _, value: _ } => {
                content = content.push(popup::edit(state));
            },
            EditingState::Delete { key: _, value: _ } => {
                content = content.push(popup::delete(state));
            }
        }
        content = content.push(rule::horizontal(3));
    }

    if state.authorization {
        content = content.push(
            column![
                text("Authorization"),
                text_input("Host", &state.host).width(Fill).on_input(Message::HostInputChanged),
                text_input("Login", &state.login).width(Fill).on_input(Message::LoginInputChanged),
                text_input("Password", &state.password).width(Fill).on_input(Message::PasswordInputChanged),
                button(text("Save")).on_press(Message::CloseAuth),
                rule::horizontal(3)
            ].spacing(3),
        )
    }

    if !state.error_msg.is_empty() {
        content = content.push(
            column![
                text(format!("Error: {}", state.error_msg)),
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

    for (key, value) in state.pairs.iter() {
        let syncstate_description = match state.pairs_syncstate.get(key) {
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

    if !state.authorization {
        if !state.sync_checking {
            if state.syncing {
                content = content.push(button(text("Stop synchronize").center().width(Fill)).width(Fill).on_press(Message::StopSynchronize));
            } else {
                content = content.push(button(text("Synchronize").center().width(Fill)).width(Fill).on_press(Message::Synchronize));
            }
        }

        if !state.syncing {
            if state.sync_checking {
                content = content.push(button(text("Stop checking").center().width(Fill)).width(Fill).on_press(Message::StopSynchronizeCheck));
            } else {
                content = content.push(button(text("Check").center().width(Fill)).width(Fill).on_press(Message::SynchronizeCheck));
            }
        }

        if !state.syncing && !state.sync_checking {
            content = content.push(button(text("Authorization").center().width(Fill)).width(Fill).on_press(Message::OpenAuth));
        }
    }

    content.into()
}

fn subscription(state: &AppState) -> Subscription<Message> {
    if state.syncing {
        let pairs_vec: Vec<(String, String)> = state.pairs.iter().map(|(k, v)| {(k.clone(), v.clone())}).collect();

        Subscription::run_with((state.host.to_owned(), state.login.to_owned(), state.password.to_owned(), pairs_vec), |(host, login, password, pairs_vec)| {
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
    } else if state.sync_checking {
        let pairs_vec: Vec<(String, String)> = state.pairs.iter().map(|(k, v)| {(k.clone(), v.clone())}).collect();

        Subscription::run_with((state.host.to_owned(), state.login.to_owned(), state.password.to_owned(), pairs_vec), |(host, login, password, pairs_vec)| {
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

fn main() -> iced::Result {
    iced::application(new, update, view)
    .title("filesync")
    .subscription(subscription)
    .run()
}