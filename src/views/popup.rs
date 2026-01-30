use iced::{
    Element,
    widget::{button, column, row, text, text_input},
};

use crate::{AppState, EditingState, Message};

fn input_fields(state: &'_ AppState) -> Element<'_, Message> {
    row![
        text_input("System path", &state.local_path_input)
            .on_input(Message::SystemPathInputChanged),
        text("<=>"),
        text_input("Server path", &state.remote_path_input)
            .on_input(Message::ServerPathInputChanged)
    ].spacing(8).into()
}

pub fn create(state: &'_ AppState) -> Element<'_, Message> {
    column![
        text("Creating pair"),
        input_fields(state),
    ].spacing(3).into()
}

pub fn edit(state: &'_ AppState) -> Element<'_, Message> {
    column![
        text("Editing pair"),
        input_fields(state),
    ].spacing(3).into()
}

pub fn delete(state: &'_ AppState) -> Element<'_, Message> {
    if let Some(EditingState::Delete { key, value }) = &state.editing {
        column![
            text("Are you sure to delete this pair?"),
            text(format!("{key} <=> {value}")),
        ].spacing(3).into()
    } else {
        column![
            text("Error: not deleting state"),
            button(text("close")).on_press(Message::DeclineEditing)
        ].into()
    }
}
