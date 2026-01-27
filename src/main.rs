use std::collections::HashMap;

use iced::{Element, Task, widget::{button, column, row, text, rule, text_input, scrollable}, Fill};

#[derive(Debug)]
enum EditingState {
    Edit,
    Delete
}

#[derive(Debug, Default)]
struct AppState {
    system_path_input: String,
    server_path_input: String,
    pairs: HashMap<String, String>,
    editing_pair: Option<(String, String, EditingState)>
}

#[derive(Debug, Clone)]
enum Message {
    SystemPathInputChanged(String),
    ServerPathInputChanged(String),
    CreatePair,
    EditPair(String),
    DeletePair(String),
    AcceptEditing,
    DeclineEditing
}

fn new() -> AppState {
    AppState { 
        system_path_input: "".to_string(),
        server_path_input: "".to_string(),
        pairs: HashMap::new(),
        editing_pair: None
    }
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
            state.editing_pair = Some(("".to_string(), "".to_string(), EditingState::Edit));
            Task::none()
        },
        Message::EditPair(key) => {
            let value: String = state.pairs.remove(&key).unwrap();
            state.system_path_input = key.clone();
            state.server_path_input = value.clone();
            state.editing_pair = Some((key, value, EditingState::Edit));
            Task::none()
        },
        Message::DeletePair(key) => {
            let value: String = state.pairs.remove(&key).unwrap();
            state.editing_pair = Some((key, value, EditingState::Delete));
            Task::none()
        },
        Message::AcceptEditing => {
            if let Some((key, value, EditingState::Edit)) = &state.editing_pair {
                if state.system_path_input != "" && state.server_path_input != "" {
                    state.pairs.insert(state.system_path_input.clone(), state.server_path_input.clone());
                } else if state.system_path_input != "" || state.server_path_input != "" {
                    state.pairs.insert(key.clone(), value.clone());
                }
            }
            state.system_path_input.clear();
            state.server_path_input.clear();
            state.editing_pair = None;
            Task::none()
        },
        Message::DeclineEditing => {
            if let Some((key, value, _editing_state)) = &state.editing_pair {
                if key != "" || value != "" {
                    state.pairs.insert(key.clone(), value.clone());
                }
            }
            state.system_path_input.clear();
            state.server_path_input.clear();
            state.editing_pair = None;
            Task::none()
        }
    }
}

fn view(state: &'_ AppState) -> Element<'_, Message> {
    let mut content = column!().spacing(8).padding(8);

    if let Some((key, value, editing_state)) = &state.editing_pair {
        match editing_state {
            EditingState::Delete => {
                content = content.push(
                    column![
                        text("Are you sure to delete pair?"),
                        row![
                            text(key),
                            text("<=>"),
                            text(value)
                        ].spacing(8),
                        row![
                            button(text("Yes")).on_press(Message::AcceptEditing),
                            button(text("No")).on_press(Message::DeclineEditing)
                        ].spacing(8),
                        rule::horizontal(3)
                    ].spacing(8)
                );
            },
            EditingState::Edit => {
                content = content.push(
                    column![
                        text("Editing"),
                        row![
                            text_input("System path", &state.system_path_input)
                                .on_input(Message::SystemPathInputChanged),
                            text("<=>"),
                            text_input("Server path", &state.server_path_input)
                                .on_input(Message::ServerPathInputChanged)
                        ].spacing(8),
                        row![
                            button(text("Save")).on_press(Message::AcceptEditing),
                            button(text("Cancel")).on_press(Message::DeclineEditing)
                        ].spacing(8),
                        rule::horizontal(3)
                    ].spacing(8)
                );
            }
        }
    }

    content = content.push(
        button(text("New pair")).on_press(Message::CreatePair)
    );

    let mut pairs_content = column!().spacing(2);
    
    for (key, value) in state.pairs.iter() {
        pairs_content = pairs_content.push(
            row![
                text(key),
                text("<=>"),
                text(value).width(Fill),
                button(text("Edit")).on_press(Message::EditPair(key.clone())),
                button(text("Delete")).on_press(Message::DeletePair(key.clone()))
            ].spacing(8)
        );
    }

    content = content.push(scrollable(pairs_content));

    content.into()
}

fn main() -> iced::Result {
    iced::application(new, update, view)
        .theme(|_s: &AppState| iced::Theme::KanagawaDragon)
        .title("filesync")
        .run()
}
