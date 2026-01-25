use std::path::PathBuf;

use iced::{Element, Task, widget::{button, column, text}, window};

#[derive(Debug, Default)]
struct AppState {
    current_dir: PathBuf
}

#[derive(Debug, Clone)]
enum Message {
    Exit
}

fn new() -> AppState {
    AppState { current_dir: std::env::current_dir().unwrap() }
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::Exit => window::latest().and_then(window::close)
    }
}

fn view(state: &'_ AppState) -> Element<'_, Message> {
    column![
        text(state.current_dir.to_str().unwrap_or("cringe"))
            .size(24),
        button(text("Exit")).on_press(Message::Exit)
    ].into()
}

fn main() -> iced::Result {
    iced::application(new, update, view)
        .theme(|_s: &AppState| iced::Theme::KanagawaDragon)
        .run()
}
