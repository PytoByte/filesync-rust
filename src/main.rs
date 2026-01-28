use std::collections::HashMap;

use iced::{Element, Task, widget::{button, column, row, text, rule, scrollable}, Fill};
use redb::{Database, Error, ReadableDatabase, ReadableTable, TableDefinition};

mod view;
mod state;

use state::{AppState, EditingState, Message};

use crate::view::popup;

const TABLE: TableDefinition<&str, &str> = TableDefinition::new("syncpairs");
const DB_PATH: &str = "./syncpairs";

fn write_db(key: &str, value: &str) -> Result<(), Error> {
    let db = Database::create(DB_PATH)?;
    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(TABLE)?;
        table.insert(key, value)?;
    }
    write_txn.commit()?;

    Ok(())
}

fn delete_db(key: &str) -> Result<(), Error> {
    let db = Database::create(DB_PATH)?;
    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(TABLE)?;
        table.remove(key)?;
    }
    write_txn.commit()?;

    Ok(())
}

fn read_db_as_hashmap() -> Result<HashMap<String, String>, Error> {
    let db = Database::open(DB_PATH)?;
    let txn = db.begin_read()?;
    let table = txn.open_table(TABLE)?;
    
    table.iter()?
        .map(|item| {
            let (key, value) = item?;
            Ok((key.value().to_string(), value.value().to_string()))
        })
        .collect()
}

fn new() -> AppState {
    let readed_table = read_db_as_hashmap();

    match readed_table {
        Ok(syncpairs) => {
            AppState {
                error_msg: String::new(),
                system_path_input: String::new(),
                server_path_input: String::new(),
                pairs: syncpairs,
                editing: None
            }
        },
        Err(e) => {
            AppState {
                error_msg: e.to_string(),
                system_path_input: String::new(),
                server_path_input: String::new(),
                pairs: HashMap::new(),
                editing: None
            }
        }
    }

    
}

fn decline_editing(state: &mut AppState) {
    if let Some(editing) = &state.editing {
        match editing {
            EditingState::Create => {},
            EditingState::Edit { key, value } | EditingState::Delete { key, value } => {
                state.pairs.insert(key.clone(), value.clone());
            },
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
        },
        Message::ServerPathInputChanged(input) => {
            state.server_path_input = input;
            Task::none()
        },
        Message::CreatePair => {
            state.editing = Some(EditingState::Create);
            Task::none()
        },
        Message::EditPair(key) => {
            if let Some(value) = state.pairs.remove(&key) {
                state.system_path_input = key.clone();
                state.server_path_input = value.clone();
                state.editing = Some(EditingState::Edit{key: key, value: value});
            }
            Task::none()
        },
        Message::DeletePair(key) => {
            if let Some(value) = state.pairs.remove(&key) {
                state.editing = Some(EditingState::Delete{key: key, value: value});
            }
            Task::none()
        },
        Message::AcceptEditing => {
            if let Some(editing) = &state.editing {
                match editing {
                    EditingState::Create | EditingState::Edit { key: _, value: _ } => {
                        if !state.system_path_input.is_empty() && !state.server_path_input.is_empty() {
                            match write_db(&state.system_path_input, &state.server_path_input) {
                                Ok(_) => {
                                    state.pairs.insert(state.system_path_input.clone(), state.server_path_input.clone());
                                },
                                Err(e) => {
                                    decline_editing(state);
                                    state.error_msg = e.to_string();
                                }
                            }
                        }
                    },
                    EditingState::Delete { key, value: _ } => {
                        if let Err(e) = delete_db(&key) {
                            decline_editing(state);
                            state.error_msg = e.to_string();
                        }
                    }
                }
            } 
            clear_editing(state);
            Task::none()
        },
        Message::DeclineEditing => {
            decline_editing(state);
            clear_editing(state);
            Task::none()
        },
        Message::CloseError => {
            state.error_msg.clear();
            Task::none()
        }
    }
}

fn view(state: &'_ AppState) -> Element<'_, Message> {
    let mut content = column!().spacing(8).padding(8);

    if let Some(editing) = &state.editing {
        match editing {
            EditingState::Create => {
                content = content.push(popup::create(state))
            },
            EditingState::Edit {key: _, value: _} => {
                content = content.push(popup::edit(state));
            },
            EditingState::Delete {key: _, value: _} => {
                content = content.push(popup::delete(state));
            }
        }
        content = content.push(rule::horizontal(3));
    }

    if !state.error_msg.is_empty() {
        content = content.push(column![
            text(format!("Error: {}", state.error_msg)),
            button(text("Close")).on_press(Message::CloseError)
        ].spacing(3));
        content = content.push(rule::horizontal(3));
    }

    content = content.push(
        button(text("New pair").center().width(Fill))
            .width(Fill)
            .on_press(Message::CreatePair)
    );

    let mut pairs_content = column!().spacing(2);
    
    for (key, value) in state.pairs.iter() {
        pairs_content = pairs_content.push(
            row![
                text(format!("{key} <=> {value}")).width(Fill),
                button(text("Edit")).on_press(Message::EditPair(key.clone())),
                button(text("Delete")).on_press(Message::DeletePair(key.clone()))
            ].spacing(8)
        );
    }

    content = content.push(scrollable(pairs_content).height(Fill));

    content = content.push(
        button(text("Synchronize").center().width(Fill))
            .width(Fill)
    );

    content.into()
}

fn main() -> iced::Result {
    iced::application(new, update, view)
        .title("filesync")
        .run()
}
