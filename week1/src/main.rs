use std::io;
use serde::Deserialize;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
    DefaultTerminal, Frame,
};
use color_eyre::{
    eyre::WrapErr,
    Result,
};

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let mut terminal = ratatui::init();
    let app_result = App::default().run(&mut terminal);

    ratatui::restore();

    app_result
}

#[derive(Debug)]
pub struct GithubClient {
    // token: String,
    client: reqwest::blocking::Client,
}

impl GithubClient {
    pub fn new(token: String) -> Self {
        Self {
            // token,
            client: reqwest::blocking::Client::new(),
        }
    }
    pub fn get_repos(&self, username: &str) -> Result<Vec<Repo>> {
        let url = format!("https://api.github.com/users/{username}/repos");
        
        let response = self.client
            .get(&url)
            .header("User-Agent", "rust-tui")
            .send()?;
    
        // println!("Status: {}", response.status());
    
        let text = response.text()?;
        // println!("Body:\n{}", text);
    
        let repos: Vec<Repo> = serde_json::from_str(&text)?;
    
        Ok(repos)
    }
}

#[derive(Debug, Deserialize)]
pub struct Repo {
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            github: GithubClient::new(String::new()),
            state: AppState::default(),
            input: String::new(),
            repos: Vec::new(),
            selected: 0,
            username: String::new(),
            exit: false,
        }
    }
}

/// Appstate
#[derive(Debug)]
pub struct App {
    github: GithubClient,
    state: AppState,
    input: String,
    repos: Vec<Repo>,
    selected: usize,
    username: String,
    exit: bool,
}

#[derive(Debug)]
pub enum AppState {
    EnterUsername,
    ShowRepos,
}
impl Default for AppState {
    fn default() -> Self {
        Self::EnterUsername
    }
}

impl App {

    /// runs the application's main loop until the user quits
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> color_eyre::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events().wrap_err("handle events failed")?;
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    fn handle_events(&mut self) -> color_eyre::Result<()> {
        match event::read()? {
            // it's important to check that the event is a key press event as
            // crossterm also emits key release and repeat events on Windows.
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => self
                .handle_key_event(key_event)
                .wrap_err_with(|| format!("handling key event failed:\n{key_event:#?}")),
            _ => Ok(()),
        }?;
        Ok(())
    }
    fn handle_key_event(&mut self, key_event: KeyEvent) -> color_eyre::Result<()> {
        match self.state {
            AppState::EnterUsername => match key_event.code {
                KeyCode::Char(c) => self.input.push(c),
                KeyCode::Backspace => { self.input.pop(); }
                KeyCode::Enter => {
                    // fetch repos
                    match self.github.get_repos(&self.input) {
                        Ok(repos) => {
                            self.repos = repos;
                            self.selected = 0;
                            self.state = AppState::ShowRepos;
                        }
                        Err(e) => eprintln!("Failed to fetch repos: {e}"),
                    }
                }
                KeyCode::Esc => self.exit = true,
                _ => {}
            },
            AppState::ShowRepos => match key_event.code {
                KeyCode::Up => {
                    if self.selected > 0 { self.selected -= 1; }
                }
                KeyCode::Down => {
                    if self.selected + 1 < self.repos.len() { self.selected += 1; }
                }
                KeyCode::Esc => {
                    self.state = AppState::EnterUsername;
                    self.input.clear();
                }
                KeyCode::Char('q') => self.exit(),
                _ => {}
            }
        }
        Ok(())
    }
    
    fn exit(&mut self) {
        self.exit = true;
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.state {
            AppState::EnterUsername => {
                let title = Line::from("Enter GitHub username:".bold());
                let input_line = Line::from(self.input.clone().yellow());
                
                let block = Block::bordered().title(title.centered()).border_set(border::THICK);
                
                Paragraph::new(Text::from(vec![input_line])).block(block).render(area, buf);
            }
            AppState::ShowRepos => {
                let title = Line::from("Repositories:".bold());
                let lines: Vec<Line> = self.repos.iter().enumerate().map(|(i, repo)| {
                    let text = if i == self.selected {
                        repo.name.clone().bold().green()
                    } else {
                        repo.name.clone().white()
                    };
                    Line::from(text)
                }).collect();
                
                let block = Block::bordered().title(title.centered()).border_set(border::THICK);
                Paragraph::new(Text::from(lines)).block(block).render(area, buf);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Style;

    #[test]
    fn render() {
        let app = App::default();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 4));

        app.render(buf.area, &mut buf);

        let mut expected = Buffer::with_lines(vec![
            "┏━━━━━━━━━━━━━ Counter App Tutorial ━━━━━━━━━━━━━┓",
            "┃                    Value: 0                    ┃",
            "┃                                                ┃",
            "┗━ Decrement <Left> Increment <Right> Quit <Q> ━━┛",
        ]);
        let title_style = Style::new().bold();
        let counter_style = Style::new().yellow();
        let key_style = Style::new().blue().bold();
        expected.set_style(Rect::new(14, 0, 22, 1), title_style);
        expected.set_style(Rect::new(28, 1, 1, 1), counter_style);
        expected.set_style(Rect::new(13, 3, 6, 1), key_style);
        expected.set_style(Rect::new(30, 3, 7, 1), key_style);
        expected.set_style(Rect::new(43, 3, 4, 1), key_style);

        assert_eq!(buf, expected);
    }
    
    // #[test]
    // fn handle_key_event() {
    //     let mut app = App::default();
    //     app.handle_key_event(KeyCode::Right.into()).unwrap();
    //     assert_eq!(app.counter, 1);

    //     app.handle_key_event(KeyCode::Left.into()).unwrap();
    //     assert_eq!(app.counter, 0);

    //     let mut app = App::default();
    //     app.handle_key_event(KeyCode::Char('q').into()).unwrap();
    //     assert!(app.exit);
    // }
    
    #[test]
    #[should_panic(expected = "attempt to subtract with overflow")]
    fn handle_key_event_panic() {
        let mut app = App::default();
        let _ = app.handle_key_event(KeyCode::Left.into());
    }
    
    #[test]
    fn handle_key_event_overflow() {
        let mut app = App::default();
        assert!(app.handle_key_event(KeyCode::Right.into()).is_ok());
        assert!(app.handle_key_event(KeyCode::Right.into()).is_ok());
        assert_eq!(
            app.handle_key_event(KeyCode::Right.into())
                .unwrap_err()
                .to_string(),
            "counter overflow"
            );
        }
}