use color_eyre::{Result, eyre::WrapErr};
use chrono::{DateTime, Utc};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf};

type RepoMap = HashMap<String, Vec<Repo>>;

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
    cache_path: PathBuf,
}

impl GithubClient {
    pub fn new() -> Self {
        let cache_path = dirs::cache_dir()
            .unwrap_or_default()
            .join("github-tui")
            .join("repo_cache.json");

        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        Self {
            // token,
            client: reqwest::blocking::Client::new(),
            cache_path,
        }
    }
    fn load_cache(&self) -> RepoMap {
        if let Ok(data) = fs::read_to_string(&self.cache_path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HashMap::new()
        }
    }
    fn save_cache(&self, cache: &RepoMap) -> Result<()> {
        let json = serde_json::to_string_pretty(cache)?;
        fs::write(&self.cache_path, json)?;
        Ok(())
    }
    pub fn get_repos(&self, username: &str, force_refresh: bool) -> Result<Vec<Repo>> {
        let mut cache = self.load_cache();
        if !force_refresh {
            if let Some(repos) = cache.get(username) {
                return Ok(repos.clone());
            }
        }

        let url = format!("https://api.github.com/users/{username}/repos");

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "github-tui")
            .send()?;

        // println!("Status: {}", response.status());

        let text = response.text()?;
        let mut repos: Vec<Repo> = serde_json::from_str(&text)?;
        for repo in &mut repos {
            repo.refresh_timestamp();
        }

        cache.insert(username.to_string(), repos.clone());
        self.save_cache(&cache)?;

        Ok(repos)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Repo {
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub html_url: String,
    pub fork: bool,
    pub language: Option<String>,
    pub forks_count: u64,
    pub stargazers_count: u64,
    pub watchers_count: u64,
    pub size: u64,
    
    /// Timestamp of last refresh in RFC3339 format
    #[serde(default)]
    pub last_updated: Option<String>,
}
impl Repo {
    pub fn refresh_timestamp(&mut self) {
        self.last_updated = Some(Utc::now().to_rfc3339().to_string());
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            github: GithubClient::new(),
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
    Loading,
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
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Enter => {
                    // fetch repos
                    if self.input.is_empty() {
                        return Ok(());
                    }
                    self.username = self.input.clone();
                    match self.github.get_repos(&self.username, false) {
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
            AppState::Loading => match key_event.code {
                KeyCode::Char('q') => self.exit(),
                _ => {}
            },
            AppState::ShowRepos => match key_event.code {
                KeyCode::Up => {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                }
                KeyCode::Down => {
                    if self.selected + 1 < self.repos.len() {
                        self.selected += 1;
                    }
                }
                KeyCode::Esc => {
                    self.state = AppState::EnterUsername;
                    self.input.clear();
                }
                KeyCode::Char('r') => {
                    if !self.username.is_empty() {
                        match self.github.get_repos(&self.username, true) {
                            Ok(repos) => {
                                self.repos = repos;
                                self.selected = 0;
                            }
                            Err(e) => eprintln!("Failed to fetch repos: {e}"),
                        }
                    }
                }
                KeyCode::Char('q') => self.exit(),
                _ => {}
            },
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
                let instructions = Line::from(vec![
                    " Submit ".into(),
                    "<enter>".blue().bold(),
                    " Quit ".into(),
                    "<q/esc>".blue().into(),
                ]);
                let block = Block::bordered()
                    .title(title.centered())
                    .title_bottom(instructions.centered())
                    .border_set(border::THICK);

                let input_line = format!("> {}_", self.input);
                Paragraph::new(input_line.yellow())
                    .block(block)
                    .render(area, buf);
            }
            AppState::Loading => {
                let block = Block::bordered()
                    .title(Line::from("Loading".bold()).centered())
                    .border_set(border::THICK);
                Paragraph::new("Fetching repositories...")
                    .centered()
                    .block(block)
                    .render(area, buf);
            }
            AppState::ShowRepos => {
                let height = area.height as usize - 2; // two rows for title and instructions
                let selected = self.selected;

                let start = if selected >= height {
                    selected + 1 - height
                } else {
                    0
                };
                let end = usize::min(start + height, self.repos.len());

                let visible_repos = &self.repos[start..end];

                let title = Line::from("Repositories".bold());

                let instructions = Line::from(vec![
                    " Move ".into(),
                    "<up/down>".blue().bold(),
                    " Refresh ".into(),
                    "<r>".blue().into(),
                    " Back ".into(),
                    "<esc>".blue().into(),
                    " Quit ".into(),
                    "<q>".blue().into(),
                ]);
                let lines: Vec<Line> = visible_repos
                    .iter()
                    .enumerate()
                    .map(|(i, repo)| {
                        let abs_idx = start + i;
                        let text = if abs_idx == self.selected {
                            repo.name.clone().bold().green()
                        } else {
                            repo.name.clone().white()
                        };
                        Line::from(text)
                    })
                    .collect();
                let lines = if visible_repos.is_empty() {
                    vec![Line::from(
                        "No repositories found. Try someone else?".yellow(),
                    )]
                } else {
                    lines
                };

                let block = Block::bordered()
                    .title(title.centered())
                    .title_bottom(instructions.centered())
                    .border_set(border::THICK);
                Paragraph::new(Text::from(lines))
                    .block(block)
                    .render(area, buf);
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
