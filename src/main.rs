use iced::{Element, Task, widget::{button, column, row, text, rule, scrollable}, Fill};

mod state;
mod popup;
use state::{AppState, EditingState, Message};

fn new() -> AppState {
    AppState::default()
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
                            state.pairs.insert(state.system_path_input.clone(), state.server_path_input.clone());
                        }
                    },
                    EditingState::Delete { key: _, value: _ } => {}
                }
            } 
            state.system_path_input.clear();
            state.server_path_input.clear();
            state.editing = None;
            Task::none()
        },
        Message::DeclineEditing => {
            if let Some(editing) = &state.editing {
                match editing {
                    EditingState::Create => {},
                    EditingState::Edit { key, value } | EditingState::Delete { key, value } => {
                        state.pairs.insert(key.clone(), value.clone());
                    },
                }
            }
            state.system_path_input.clear();
            state.server_path_input.clear();
            state.editing = None;
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
