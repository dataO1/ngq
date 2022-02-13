use crate::core::player::{self, PreviewBuffer};
use crossterm::{
    event::{self, EnableMouseCapture, KeyCode},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use std::{
    io,
    sync::{Arc, Mutex},
};
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};
use tui::backend::{Backend, CrosstermBackend};
use tui::{
    layout::{Constraint, Direction, Layout},
    Frame, Terminal,
};

use crate::core::player::{Message, Player};

use super::widgets::preview::{PreviewType, PreviewWidget};

#[derive(Clone, Debug)]
pub enum Event {
    TogglePlay,
    LoadTrack(String),
    Quit,
    Unknown,
}
/// Represents the App's State
pub struct AppState {}

impl Default for AppState {
    fn default() -> AppState {
        AppState {}
    }
}

pub struct App {
    frame_buf: Arc<Mutex<PreviewBuffer>>,
    player_position: usize,
}

impl Default for App {
    fn default() -> Self {
        Self {
            frame_buf: Arc::new(Mutex::new(PreviewBuffer::default())),
            player_position: 0,
        }
    }
}

impl App {
    pub async fn run(mut self) -> io::Result<()> {
        // init terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        // create all message passing channels
        let (key_events_out, mut key_events_in) = channel::<Event>(10);
        let (player_events_out, mut player_events_in) = channel::<player::Event>(1);
        let (player_messages_out, player_messages_in) = channel::<player::Message>(1);
        // spawn the input thread
        let _kb_join_handle = App::spawn_key_handler(key_events_out.clone());
        let player_handle = Player::spawn(
            player_messages_in,
            player_events_out,
            Arc::clone(&self.frame_buf),
        );
        // execute main UI loop
        loop {
            // draw to terminal
            terminal.draw(|f| self.layout(f))?;
            // // get events async
            // if let Some(ev) = self.event_channel_rx.recv().await {
            //     // update state
            //     self.update(ev).await;
            // }
            // get events async
            // update state
            self.update(
                &mut key_events_in,
                player_messages_out.clone(),
                &mut player_events_in,
            )
            .await;
        }
    }

    fn spawn_key_handler(app: Sender<Event>) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if let crossterm::event::Event::Key(key) = event::read().unwrap() {
                    let ev = match key.code {
                        KeyCode::Enter => Event::LoadTrack(String::from("music/bass_symptom.mp3")),
                        KeyCode::Char(' ') => Event::TogglePlay,
                        KeyCode::Char('q') => Event::Quit,
                        _ => Event::Unknown,
                    };
                    match app.send(ev).await {
                        Ok(_res) => (),
                        Err(err) => {
                            println!("Error:{:#?}", err)
                        }
                    }
                };
            }
        })
    }

    ///update the app's model
    async fn update(
        &mut self,
        key_events_in: &mut Receiver<Event>,
        player_messages_out: Sender<player::Message>,
        player_events_in: &mut Receiver<player::Event>,
    ) -> () {
        if let Ok(ev) = key_events_in.try_recv() {
            match ev {
                Event::TogglePlay => {
                    player_messages_out.send(Message::TogglePlay).await;
                }
                Event::LoadTrack(track) => {
                    player_messages_out.send(Message::Load(track)).await;
                }
                Event::Quit => std::process::exit(0),
                Event::Unknown => {
                    //ignore unknown commands
                }
            }
        };
        if let Ok(ev) = player_events_in.try_recv() {
            match ev {
                player::Event::PlayedPackage(num_packets) => {
                    self.player_position += num_packets;
                }
                _ => {}
            }
        }
    }

    /// define how the app should look like
    fn layout<B: Backend>(&mut self, f: &mut Frame<B>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Percentage(5),
                    Constraint::Percentage(30),
                    Constraint::Percentage(65),
                ]
                .as_ref(),
            )
            .split(f.size());
        let live_preview = PreviewWidget::new(
            PreviewType::LivePreview,
            Arc::clone(&self.frame_buf),
            self.player_position,
        );
        let preview = PreviewWidget::new(
            PreviewType::Preview,
            Arc::clone(&self.frame_buf),
            self.player_position,
        );

        f.render_widget(preview, chunks[0]);
        f.render_widget(live_preview, chunks[1]);
    }
}
