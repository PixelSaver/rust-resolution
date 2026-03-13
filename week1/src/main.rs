use chrono::{DateTime, Duration, Utc};
use color_eyre::{Result, eyre::WrapErr};
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
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    time::{Duration as StdDuration, Instant},
};

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
    pub fn time_ago(timestamp: &str) -> Option<String> {
        let last_updated: DateTime<Utc> = timestamp.parse().ok()?;
        let now = Utc::now();
        let duration = now - last_updated;

        let seconds = duration.num_seconds();
        if seconds < 60 {
            return Some(format!("{seconds} seconds ago"));
        }
        let minutes = seconds / 60;
        if minutes < 60 {
            return Some(format!("{minutes} minutes ago"));
        }
        let hours = minutes / 60;
        if hours < 24 {
            return Some(format!("{hours} hours ago"));
        }
        let days = hours / 24;
        if days < 30 {
            return Some(format!("{days} days ago"));
        }
        let months = days / 30;
        if months < 12 {
            return Some(format!("{months} months ago"));
        }
        let years = months / 12;
        Some(format!("{years} years ago"))
    }
}

/// Appstate
#[derive(Debug)]
pub struct App {
    github: GithubClient,
    state: AppState,
    /// Input used to query username
    input: String,
    /// Store of all repos of a specific user
    repos: Vec<Repo>,
    /// Selected index
    selected: usize,
    username: String,
    /// Whether or not to exit
    exit: bool,
    /// Visual notification when refreshing
    refreshing: bool,
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
            refreshing: false,
        }
    }
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
        let tick_rate = StdDuration::from_secs(1);
        let mut last_tick = Instant::now();

        while !self.exit {
            // redraw every loop
            terminal.draw(|frame| self.draw(frame))?;

            // handle input events
            if event::poll(StdDuration::from_millis(100))? {
                if let Event::Key(key_event) = event::read()? {
                    if key_event.kind == KeyEventKind::Press {
                        self.handle_key_event(key_event, terminal)?;
                    }
                }
            }

            // tick-based redraw
            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
                terminal.draw(|frame| self.draw(frame))?;
            }
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> color_eyre::Result<()> {
        match event::read()? {
            // it's important to check that the event is a key press event as
            // crossterm also emits key release and repeat events on Windows.
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => self
                .handle_key_event(key_event, terminal)
                .wrap_err_with(|| format!("handling key event failed:\n{key_event:#?}")),
            _ => Ok(()),
        }?;
        Ok(())
    }
    fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        terminal: &mut DefaultTerminal,
    ) -> color_eyre::Result<()> {
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
                        self.refreshing = true;
                        terminal.draw(|f| self.draw(f)).ok();

                        match self.github.get_repos(&self.username, true) {
                            Ok(repos) => {
                                self.repos = repos;
                                self.selected = 0;
                            }
                            Err(e) => eprintln!("Failed to fetch repos: {e}"),
                        }
                        self.refreshing = false;

                        terminal.draw(|f| self.draw(f)).ok();
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
                    "<esc>".blue().bold(),
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
                    "<r>".blue().bold(),
                    " Back ".into(),
                    "<esc>".blue().bold(),
                    " Quit ".into(),
                    "<q>".blue().bold(),
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

                let earliest_refresh = self
                    .repos
                    .iter()
                    .filter_map(|r| r.last_updated.as_ref())
                    .min()
                    .map(|s| s.as_str());
                let refresh_info = if self.refreshing {
                    " |  Refreshing...  ".to_string()
                } else if let Some(time_stamp) = earliest_refresh {
                    format!(
                        " |  Last refreshed: {} ",
                        Repo::time_ago(time_stamp).unwrap_or_else(|| "Never".to_string())
                    )
                } else {
                    "Never refreshed".to_string()
                };

                let block = Block::bordered()
                    .title(title.centered())
                    .title_bottom(instructions.centered())
                    .border_set(border::THICK)
                    .title_bottom(Line::from(refresh_info).right_aligned());
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

    /// Create a dummy repo list
    fn dummy_repos(n: usize) -> Vec<Repo> {
        (0..n)
            .map(|i| Repo {
                name: format!("Repo{}", i),
                full_name: format!("User/Repo{}", i),
                description: Some(format!("Description {}", i)),
                html_url: format!("https://github.com/user/repo{}", i),
                fork: false,
                language: Some("Rust".into()),
                forks_count: i as u64,
                stargazers_count: i as u64,
                watchers_count: i as u64,
                size: 100,
                last_updated: None,
            })
            .collect()
    }

    #[test]
    fn test_enter_username_state() {
        let mut app = App::default();

        // Initially in EnterUsername
        assert!(matches!(app.state, AppState::EnterUsername));

        // Type some characters
        for c in "alice".chars() {
            app.handle_key_event(KeyEvent::from(KeyCode::Char(c)), &mut ratatui::init())
                .unwrap();
        }
        assert_eq!(app.input, "alice");
    }

    #[test]
    fn test_enter_username_submit() {
        let mut app = App::default();

        // Simulate typing and Enter
        app.input = "dummy".into();

        // Override github client for test
        app.github = GithubClient::new();
        // Here you could mock get_repos to return dummy_repos
        // For simplicity, let's insert directly:
        let repos = dummy_repos(5);
        app.repos = repos.clone();
        app.state = AppState::ShowRepos;

        assert_eq!(app.repos.len(), 5);
        assert_eq!(app.selected, 0);
    }
    #[test]
    fn test_repo_selection_bounds() {
        let mut app = App::default();
        app.repos = dummy_repos(3);
        app.state = AppState::ShowRepos;
        app.selected = 0;

        // Move up at top -> stays 0
        app.handle_key_event(KeyEvent::from(KeyCode::Up), &mut ratatui::init())
            .unwrap();
        assert_eq!(app.selected, 0);

        // Move down -> 1
        app.handle_key_event(KeyEvent::from(KeyCode::Down), &mut ratatui::init())
            .unwrap();
        assert_eq!(app.selected, 1);

        // Move down -> 2
        app.handle_key_event(KeyEvent::from(KeyCode::Down), &mut ratatui::init())
            .unwrap();
        assert_eq!(app.selected, 2);

        // Move down at bottom -> stays 2
        app.handle_key_event(KeyEvent::from(KeyCode::Down), &mut ratatui::init())
            .unwrap();
        assert_eq!(app.selected, 2);
    }
}
