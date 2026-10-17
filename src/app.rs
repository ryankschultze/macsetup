//! Dashboard state and key handling. Pure: no terminal, no processes. The
//! main loop feeds it keys and engine messages and starts the jobs it asks for.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::{self, Source};
use crate::engine::{Envelope, Msg};
use crate::events::{Item, ItemState};
use crate::model::{Counts, JobKind, Model};
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Packages,
    Dotfiles,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Overview, Tab::Packages, Tab::Dotfiles];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Packages => "Packages",
            Tab::Dotfiles => "Dotfiles",
        }
    }

    fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    fn step(self, by: isize) -> Tab {
        let n = Tab::ALL.len() as isize;
        Tab::ALL[((self.index() as isize + by).rem_euclid(n)) as usize]
    }
}

/// The first-run screen: no engine found, offer to save one in config.toml.
#[derive(Debug, Clone)]
pub struct FirstRun {
    pub input: String,
    pub error: Option<String>,
    pub config_file: PathBuf,
}

#[derive(Debug, Clone)]
pub enum Screen {
    FirstRun(FirstRun),
    /// Something that stops the dashboard: a bad config, an API mismatch.
    Fatal {
        title: String,
        body: String,
    },
    Main,
}

/// A row in the Packages tree.
#[derive(Debug, Clone, Copy)]
pub enum Row<'a> {
    Group {
        id: &'a str,
        expanded: bool,
        counts: Counts,
    },
    Item(&'a Item),
}

pub struct App {
    pub screen: Screen,
    pub tab: Tab,
    pub model: Model,
    pub theme: Theme,
    pub home: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    pub repo_source: Option<Source>,
    pub api: Option<u32>,
    pub running: Option<(u64, JobKind)>,
    pub queue: VecDeque<JobKind>,
    next_job: u64,
    pub spinner: usize,
    pub last_error: Option<String>,
    pub show_help: bool,
    pub quit: bool,
    /// Set by the first-run screen once config.toml is written; the main loop
    /// then checks the engine and connects.
    pub after_first_run: Option<PathBuf>,
    // Packages tab
    pub expanded: BTreeSet<String>,
    pub pkg_sel: usize,
    pub state_filter: Option<ItemState>,
    pub text_filter: String,
    pub editing_filter: bool,
    // Dotfiles tab
    pub dot_sel: usize,
}

pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

impl App {
    pub fn new(screen: Screen, theme: Theme, home: Option<PathBuf>) -> Self {
        App {
            screen,
            tab: Tab::Overview,
            model: Model::default(),
            theme,
            home,
            repo: None,
            repo_source: None,
            api: None,
            running: None,
            queue: VecDeque::new(),
            next_job: 1,
            spinner: 0,
            last_error: None,
            show_help: false,
            quit: false,
            after_first_run: None,
            expanded: BTreeSet::new(),
            pkg_sel: 0,
            state_filter: None,
            text_filter: String::new(),
            editing_filter: false,
            dot_sel: 0,
        }
    }

    /// The engine was found and speaks our API: load the list, then check.
    pub fn connected(&mut self, repo: PathBuf, source: Source, api: u32) {
        self.repo = Some(repo);
        self.repo_source = Some(source);
        self.api = Some(api);
        self.screen = Screen::Main;
        self.enqueue(JobKind::List);
        self.enqueue(JobKind::Check);
    }

    pub fn fatal(&mut self, title: impl Into<String>, body: impl Into<String>) {
        self.screen = Screen::Fatal {
            title: title.into(),
            body: body.into(),
        };
    }

    /// Show `path` with the home folder as `~`.
    pub fn tilde(&self, path: &Path) -> String {
        match self.home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
            Some(rest) if rest.as_os_str().is_empty() => "~".into(),
            Some(rest) => format!("~/{}", rest.display()),
            None => path.display().to_string(),
        }
    }

    pub fn tilde_str(&self, path: &str) -> String {
        if path.is_empty() {
            return String::new();
        }
        self.tilde(Path::new(path))
    }

    // --- jobs ---------------------------------------------------------------

    /// Queue a job unless the same one is already running or queued.
    pub fn enqueue(&mut self, kind: JobKind) {
        if self.running.map(|(_, k)| k) == Some(kind) || self.queue.contains(&kind) {
            return;
        }
        self.queue.push_back(kind);
    }

    /// The next job to start, if none is running (one job at a time).
    pub fn take_next_job(&mut self) -> Option<(u64, JobKind)> {
        if self.running.is_some() {
            return None;
        }
        let kind = self.queue.pop_front()?;
        let id = self.next_job;
        self.next_job += 1;
        self.running = Some((id, kind));
        self.model.begin(kind);
        Some((id, kind))
    }

    pub fn on_engine(&mut self, env: Envelope) {
        if self.running.map(|(id, _)| id) != Some(env.job) {
            return; // a job we no longer care about
        }
        match env.msg {
            Msg::Event(e) => self.model.apply(env.kind, e),
            Msg::BadLine(l) => self.model.bad_line(env.kind, l),
            Msg::Finished(result) => {
                self.running = None;
                if let Err(e) = result {
                    self.last_error = Some(format!("{}: {e}", env.kind.label()));
                    self.model.fail(env.kind, &e);
                }
                self.clamp();
            }
        }
    }

    pub fn tick(&mut self) {
        if self.running.is_some() {
            self.spinner = (self.spinner + 1) % SPINNER.len();
        }
    }

    pub fn is_running(&self, kind: JobKind) -> bool {
        self.running.map(|(_, k)| k) == Some(kind)
    }

    // --- Packages rows ------------------------------------------------------

    pub fn filter_active(&self) -> bool {
        self.state_filter.is_some() || !self.text_filter.is_empty()
    }

    fn matches(&self, item: &Item) -> bool {
        if self.state_filter.is_some_and(|s| s != item.state) {
            return false;
        }
        if self.text_filter.is_empty() {
            return true;
        }
        let needle = self.text_filter.to_lowercase();
        [&item.name, &item.desc, &item.kind]
            .iter()
            .any(|f| f.to_lowercase().contains(&needle))
    }

    /// The visible Packages rows. With a filter active, only matching items
    /// show and every group with a match is open.
    pub fn package_rows(&self) -> Vec<Row<'_>> {
        let counts = self.model.counts();
        let filtering = self.filter_active();
        let mut rows = Vec::new();
        for id in self.model.group_order() {
            let items: Vec<&Item> = self
                .model
                .items
                .iter()
                .filter(|i| i.group == id && self.matches(i))
                .collect();
            if filtering && items.is_empty() {
                continue;
            }
            let expanded = filtering || self.expanded.contains(id);
            rows.push(Row::Group {
                id,
                expanded,
                counts: counts.get(id).copied().unwrap_or_default(),
            });
            if expanded {
                rows.extend(items.into_iter().map(Row::Item));
            }
        }
        rows
    }

    fn clamp(&mut self) {
        let n = self.package_rows().len();
        self.pkg_sel = self.pkg_sel.min(n.saturating_sub(1));
        let d = self.model.dotfiles.len();
        self.dot_sel = self.dot_sel.min(d.saturating_sub(1));
    }

    fn move_sel(&mut self, by: isize) {
        let len = match self.tab {
            Tab::Packages => self.package_rows().len(),
            Tab::Dotfiles => self.model.dotfiles.len(),
            Tab::Overview => return,
        };
        let sel = match self.tab {
            Tab::Packages => &mut self.pkg_sel,
            _ => &mut self.dot_sel,
        };
        if len == 0 {
            *sel = 0;
            return;
        }
        *sel = (*sel as isize + by).clamp(0, len as isize - 1) as usize;
    }

    fn selected_group(&self) -> Option<(String, bool)> {
        match self.package_rows().get(self.pkg_sel)? {
            Row::Group { id, expanded, .. } => Some((id.to_string(), *expanded)),
            Row::Item(item) => Some((item.group.clone(), true)),
        }
    }

    fn set_expanded(&mut self, open: Option<bool>) {
        if self.filter_active() {
            return; // every matching group is already open
        }
        let Some((gid, expanded)) = self.selected_group() else {
            return;
        };
        let open = open.unwrap_or(!expanded);
        if open {
            self.expanded.insert(gid.clone());
        } else {
            self.expanded.remove(&gid);
        }
        // keep the cursor on the group row
        if let Some(pos) = self
            .package_rows()
            .iter()
            .position(|r| matches!(r, Row::Group { id, .. } if *id == gid))
        {
            self.pkg_sel = pos;
        }
    }

    fn cycle_state_filter(&mut self) {
        self.state_filter = match self.state_filter {
            None => Some(ItemState::Missing),
            Some(ItemState::Missing) => Some(ItemState::Outside),
            Some(ItemState::Outside) => Some(ItemState::Mismatch),
            Some(ItemState::Mismatch) => Some(ItemState::Unknown),
            Some(ItemState::Unknown) => Some(ItemState::Installed),
            Some(ItemState::Installed) => None,
        };
        self.pkg_sel = 0;
    }

    // --- keys ---------------------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        match &mut self.screen {
            Screen::FirstRun(_) => self.first_run_key(key),
            Screen::Fatal { .. } => {
                if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter) {
                    self.quit = true;
                }
            }
            Screen::Main => self.main_key(key),
        }
    }

    fn first_run_key(&mut self, key: KeyEvent) {
        let Screen::FirstRun(fr) = &mut self.screen else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.quit = true,
            KeyCode::Backspace => {
                fr.input.pop();
                fr.error = None;
            }
            KeyCode::Char(c) => {
                fr.input.push(c);
                fr.error = None;
            }
            KeyCode::Enter => {
                match config::write_config(&fr.config_file, &fr.input, self.home.as_deref()) {
                    Ok(repo) => self.after_first_run = Some(repo),
                    Err(e) => fr.error = Some(e),
                }
            }
            _ => {}
        }
    }

    fn main_key(&mut self, key: KeyEvent) {
        if self.show_help {
            self.show_help = false;
            return;
        }
        if self.editing_filter {
            match key.code {
                KeyCode::Enter => self.editing_filter = false,
                KeyCode::Esc => {
                    self.text_filter.clear();
                    self.editing_filter = false;
                }
                KeyCode::Backspace => {
                    self.text_filter.pop();
                }
                KeyCode::Char(c) => self.text_filter.push(c),
                _ => {}
            }
            self.pkg_sel = 0;
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => {
                if self.tab == Tab::Packages && self.filter_active() {
                    self.state_filter = None;
                    self.text_filter.clear();
                    self.pkg_sel = 0;
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('1') => self.tab = Tab::Overview,
            KeyCode::Char('2') => self.tab = Tab::Packages,
            KeyCode::Char('3') => self.tab = Tab::Dotfiles,
            KeyCode::Tab => self.tab = self.tab.step(1),
            KeyCode::BackTab => self.tab = self.tab.step(-1),
            KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
            KeyCode::PageUp => self.move_sel(-10),
            KeyCode::PageDown => self.move_sel(10),
            KeyCode::Home | KeyCode::Char('g') => self.move_sel(isize::MIN / 2),
            KeyCode::End | KeyCode::Char('G') => self.move_sel(isize::MAX / 2),
            KeyCode::Char('r') => self.enqueue(JobKind::List),
            KeyCode::Char('c') => self.enqueue(JobKind::Check),
            _ if self.tab == Tab::Packages => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => self.set_expanded(None),
                KeyCode::Right | KeyCode::Char('l') => self.set_expanded(Some(true)),
                KeyCode::Left | KeyCode::Char('h') => self.set_expanded(Some(false)),
                KeyCode::Char('f') => self.cycle_state_filter(),
                KeyCode::Char('/') => self.editing_filter = true,
                _ => {}
            },
            _ => {}
        }
    }
}
