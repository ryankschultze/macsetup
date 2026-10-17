//! Dashboard state and key handling. Pure: no terminal, no processes. The
//! main loop feeds it keys and engine messages and starts the jobs it asks for.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::{Action, ReviewChoice};
use crate::activity::{Activity, Outcome};
use crate::config::{self, Source};
use crate::engine::{Envelope, Msg};
use crate::events::{DotfileState, Item, ItemState, Status};
use crate::model::{Counts, JobKind, Model};
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Packages,
    Capture,
    Dotfiles,
    Activity,
    Profiles,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Overview,
        Tab::Packages,
        Tab::Capture,
        Tab::Dotfiles,
        Tab::Activity,
        Tab::Profiles,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Packages => "Packages",
            Tab::Capture => "Capture",
            Tab::Dotfiles => "Dotfiles",
            Tab::Activity => "Activity",
            Tab::Profiles => "Profiles",
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

/// The group the engine always installs: shown on, never toggled.
pub const ALWAYS_ON: &str = "core";

/// Which list has the cursor on the Profiles tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfilesFocus {
    Groups,
    Profiles,
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

/// A queued or running engine command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub kind: JobKind,
    pub action: Option<Action>,
    pub dry_run: bool,
}

impl Job {
    fn data(kind: JobKind) -> Self {
        Job {
            kind,
            action: None,
            dry_run: false,
        }
    }

    /// Arguments after `setup` (the engine client adds `--json`/`--events`).
    pub fn args(&self) -> Vec<String> {
        match &self.action {
            Some(a) => a.argv(self.dry_run),
            None => self.kind.args().iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Runs attached to the terminal rather than in the background.
    pub fn attached(&self) -> bool {
        !self.dry_run && self.action.as_ref().is_some_and(Action::needs_terminal)
    }

    pub fn title(&self) -> String {
        match &self.action {
            Some(a) if self.dry_run => format!("Dry run: {}", a.title()),
            Some(a) => a.title(),
            None => match self.kind {
                JobKind::List => "Load the package list".into(),
                JobKind::Check => "Check".into(),
                JobKind::CaptureList => "Find untracked installs".into(),
                JobKind::Action => "Action".into(),
            },
        }
    }

    pub fn command(&self) -> String {
        let mut s = String::from("setup");
        for a in self.args() {
            s.push(' ');
            s.push_str(&a);
        }
        s.push_str(if self.attached() {
            " --events FILE (in the terminal)"
        } else {
            " --json"
        });
        s
    }
}

/// Things the main loop does on the app's behalf (it owns processes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Stop a background job's process group.
    Kill(u32),
    /// Put text on the clipboard.
    Copy(String),
    /// Open a file with its default app.
    Open(String),
}

/// The confirm dialog: what will run, and an optional dry-run preview.
#[derive(Debug, Clone)]
pub struct Dialog {
    pub action: Action,
    /// Some once a dry run was asked for; the job id once it started.
    pub preview: Option<Option<u64>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickFor {
    Move { id: String },
    Capture { id: String },
}

/// The group picker for move and capture.
#[derive(Debug, Clone)]
pub struct Picker {
    pub purpose: PickFor,
    pub options: Vec<String>,
    pub sel: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub text: String,
    pub status: Status,
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
    pub running_job: Option<Job>,
    /// The running background job's pid (attached jobs have none).
    pub running_pid: Option<u32>,
    cancelling: Option<u64>,
    pub queue: VecDeque<Job>,
    next_job: u64,
    pub effects: Vec<Effect>,
    pub dialog: Option<Dialog>,
    pub picker: Option<Picker>,
    pub toast: Option<Toast>,
    pub activity: Activity,
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
    // Capture tab
    pub cap_sel: usize,
    // Activity tab: None follows the newest job
    pub act_sel: Option<usize>,
    /// Lines scrolled up from the end of the selected job.
    pub act_scroll: usize,
    // Profiles tab
    pub prof_focus: ProfilesFocus,
    pub grp_sel: usize,
    pub prof_sel: usize,
    /// An unsaved group selection; None while it matches the saved one.
    pub pending_groups: Option<BTreeSet<String>>,
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
            running_job: None,
            running_pid: None,
            cancelling: None,
            queue: VecDeque::new(),
            next_job: 1,
            effects: Vec::new(),
            dialog: None,
            picker: None,
            toast: None,
            activity: Activity::default(),
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
            cap_sel: 0,
            act_sel: None,
            act_scroll: 0,
            prof_focus: ProfilesFocus::Groups,
            grp_sel: 0,
            prof_sel: 0,
            pending_groups: None,
        }
    }

    /// The engine was found and speaks our API: load the list, then check.
    pub fn connected(&mut self, repo: PathBuf, source: Source, api: u32) {
        self.repo = Some(repo);
        self.repo_source = Some(source);
        self.api = Some(api);
        self.screen = Screen::Main;
        self.enqueue(JobKind::List);
        self.enqueue(JobKind::CaptureList);
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

    /// Queue a data job (list, check, capture --list) unless the same one is
    /// already running or queued.
    pub fn enqueue(&mut self, kind: JobKind) {
        if self.running.map(|(_, k)| k) == Some(kind) || self.queued(kind) {
            return;
        }
        self.queue.push_back(Job::data(kind));
    }

    pub fn queued(&self, kind: JobKind) -> bool {
        self.queue.iter().any(|j| j.kind == kind)
    }

    /// Queue an action, or its dry run.
    pub fn enqueue_action(&mut self, action: Action, dry_run: bool) {
        self.queue.push_back(Job {
            kind: JobKind::Action,
            action: Some(action),
            dry_run,
        });
    }

    /// The next job to start, if none is running (one job at a time).
    pub fn take_next_job(&mut self) -> Option<(u64, JobKind)> {
        if self.running.is_some() {
            return None;
        }
        let job = self.queue.pop_front()?;
        let id = self.next_job;
        self.next_job += 1;
        self.running = Some((id, job.kind));
        self.model.begin(job.kind);
        self.activity
            .start(id, job.title(), job.command(), job.dry_run);
        if job.dry_run
            && let Some(d) = &mut self.dialog
            && job.action.as_ref() == Some(&d.action)
        {
            d.preview = Some(Some(id));
        }
        if job.kind == JobKind::Action && !job.dry_run {
            self.act_sel = None; // follow the action in Activity
            self.act_scroll = 0;
        }
        let kind = job.kind;
        self.running_job = Some(job);
        Some((id, kind))
    }

    pub fn running_title(&self) -> Option<String> {
        self.running_job.as_ref().map(Job::title)
    }

    pub fn on_engine(&mut self, env: Envelope) {
        if self.running.map(|(id, _)| id) != Some(env.job) {
            return; // a job we no longer care about
        }
        match env.msg {
            Msg::Event(e) => {
                self.activity.event(env.job, &e);
                self.model.apply(env.kind, e);
            }
            Msg::BadLine(l) => {
                self.activity.raw(env.job, l.clone());
                self.model.bad_line(env.kind, l);
            }
            Msg::Finished(result) => {
                let job = self.running_job.take();
                self.running = None;
                self.running_pid = None;
                let cancelled = self.cancelling.take() == Some(env.job);
                if cancelled {
                    self.activity.cancelled(env.job);
                } else {
                    self.activity
                        .finish(env.job, result.as_ref().err().map(ToString::to_string));
                }
                if let Err(e) = &result {
                    self.model.fail(env.kind, e);
                    if env.kind != JobKind::Action && !cancelled {
                        self.last_error = Some(format!("{}: {e}", env.kind.label()));
                    }
                }
                if let Some(job) = job
                    && let Some(action) = &job.action
                    && !job.dry_run
                {
                    let log = self.activity.get(env.job);
                    let quiet = log.is_some_and(|l| {
                        matches!(
                            l.outcome,
                            Some(Outcome::Done {
                                ok: 0,
                                skip: 0,
                                warn: 0,
                                manual: 0,
                                fail: 0
                            })
                        )
                    });
                    let succeeded = result.is_ok()
                        && log.is_some_and(|l| {
                            matches!(l.outcome, Some(Outcome::Done { fail: 0, .. }))
                        });
                    self.toast = log.map(|l| Toast {
                        text: match action.done_text() {
                            Some(t) if quiet => format!("{}: {t}", l.title),
                            _ => format!("{}: {}", l.title, l.summary()),
                        },
                        status: l.status(),
                    });
                    if succeeded && matches!(action, Action::SetGroups { .. } | Action::Reset) {
                        self.pending_groups = None;
                    }
                    for k in action.refresh() {
                        self.enqueue(k);
                    }
                }
                self.clamp();
            }
        }
    }

    /// The main loop started the running job in the background as `pid`.
    pub fn started(&mut self, pid: Option<u32>) {
        self.running_pid = pid;
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
        let c = self.model.untracked.len();
        self.cap_sel = self.cap_sel.min(c.saturating_sub(1));
        let g = self.model.group_order().len();
        self.grp_sel = self.grp_sel.min(g.saturating_sub(1));
        let p = self.model.profiles.len();
        self.prof_sel = self.prof_sel.min(p.saturating_sub(1));
        if self.pending_groups.as_ref() == Some(&self.saved_groups()) {
            self.pending_groups = None;
        }
    }

    /// The Activity tab's selected job index.
    pub fn activity_index(&self) -> Option<usize> {
        let n = self.activity.jobs.len();
        if n == 0 {
            return None;
        }
        Some(self.act_sel.unwrap_or(n - 1).min(n - 1))
    }

    fn move_sel(&mut self, by: isize) {
        if self.tab == Tab::Activity {
            let n = self.activity.jobs.len();
            if let Some(i) = self.activity_index() {
                let next = (i as isize + by).clamp(0, n as isize - 1) as usize;
                self.act_sel = if next == n - 1 { None } else { Some(next) };
                self.act_scroll = 0;
            }
            return;
        }
        let len = match (self.tab, self.prof_focus) {
            (Tab::Packages, _) => self.package_rows().len(),
            (Tab::Dotfiles, _) => self.model.dotfiles.len(),
            (Tab::Capture, _) => self.model.untracked.len(),
            (Tab::Profiles, ProfilesFocus::Groups) => self.model.group_order().len(),
            (Tab::Profiles, ProfilesFocus::Profiles) => self.model.profiles.len(),
            (Tab::Overview | Tab::Activity, _) => return,
        };
        let sel = match (self.tab, self.prof_focus) {
            (Tab::Packages, _) => &mut self.pkg_sel,
            (Tab::Capture, _) => &mut self.cap_sel,
            (Tab::Profiles, ProfilesFocus::Groups) => &mut self.grp_sel,
            (Tab::Profiles, ProfilesFocus::Profiles) => &mut self.prof_sel,
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
        if self.picker.is_some() {
            self.picker_key(key);
            return;
        }
        if self.dialog.is_some() {
            self.dialog_key(key);
            return;
        }
        self.toast = None;
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
                } else if self.tab == Tab::Profiles && self.pending_groups.is_some() {
                    self.pending_groups = None;
                    self.say("Unsaved group changes discarded.", Status::Skip);
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char(c @ '1'..='6') => self.tab = Tab::ALL[(c as usize) - ('1' as usize)],
            KeyCode::Tab => self.tab = self.tab.step(1),
            KeyCode::BackTab => self.tab = self.tab.step(-1),
            // On Dotfiles, k means keep: move with the arrows there.
            KeyCode::Char('k') if self.tab == Tab::Dotfiles => {
                self.review(ReviewChoice::Keep);
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
            KeyCode::PageUp if self.tab == Tab::Activity => {
                self.act_scroll = self.act_scroll.saturating_add(10);
            }
            KeyCode::PageDown if self.tab == Tab::Activity => {
                self.act_scroll = self.act_scroll.saturating_sub(10);
            }
            KeyCode::PageUp => self.move_sel(-10),
            KeyCode::PageDown => self.move_sel(10),
            KeyCode::Home | KeyCode::Char('g') => self.move_sel(isize::MIN / 2),
            KeyCode::End | KeyCode::Char('G') => self.move_sel(isize::MAX / 2),
            KeyCode::Char('r') => {
                self.enqueue(JobKind::List);
                self.enqueue(JobKind::CaptureList);
            }
            KeyCode::Char('c') => self.enqueue(JobKind::Check),
            _ => match self.tab {
                Tab::Packages => self.packages_key(key),
                Tab::Capture => {
                    if key.code == KeyCode::Char('a') {
                        self.capture_add();
                    }
                }
                Tab::Dotfiles => match key.code {
                    KeyCode::Char('t') => self.review(ReviewChoice::Take),
                    KeyCode::Char('l') => self.review(ReviewChoice::Later),
                    KeyCode::Char('m') => self.review(ReviewChoice::Merge),
                    _ => {}
                },
                Tab::Overview => {
                    if key.code == KeyCode::Char('l') {
                        self.open_log();
                    }
                }
                Tab::Activity => match key.code {
                    KeyCode::Char('x') => self.cancel(),
                    KeyCode::Char('y') => self.copy_log(),
                    _ => {}
                },
                Tab::Profiles => self.profiles_key(key),
            },
        }
    }

    fn packages_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => self.set_expanded(None),
            KeyCode::Right | KeyCode::Char('l') => self.set_expanded(Some(true)),
            KeyCode::Left | KeyCode::Char('h') => self.set_expanded(Some(false)),
            KeyCode::Char('f') => self.cycle_state_filter(),
            KeyCode::Char('/') => self.editing_filter = true,
            KeyCode::Char('i') => self.install(),
            KeyCode::Char('u') => self.on_item(|i| Action::Uninstall {
                ids: vec![i.id.clone()],
                drop: false,
            }),
            KeyCode::Char('U') => self.on_item(|i| Action::Uninstall {
                ids: vec![i.id.clone()],
                drop: true,
            }),
            KeyCode::Char('d') => self.on_item(|i| Action::Drop {
                ids: vec![i.id.clone()],
            }),
            KeyCode::Char('m') => {
                if let Some(Row::Item(item)) = self.package_rows().get(self.pkg_sel) {
                    let (id, group) = (item.id.clone(), item.group.clone());
                    self.pick(PickFor::Move { id }, Some(&group));
                } else {
                    self.say("Select a package to move.", Status::Warn);
                }
            }
            _ => {}
        }
    }

    // --- Profiles -----------------------------------------------------------

    /// The groups saved in the engine's state (`selected` in `setup list`).
    pub fn saved_groups(&self) -> BTreeSet<String> {
        self.model
            .groups
            .iter()
            .filter(|g| g.selected)
            .map(|g| g.id.clone())
            .collect()
    }

    /// What would be saved now: the pending selection, else the saved one.
    pub fn effective_groups(&self) -> BTreeSet<String> {
        self.pending_groups
            .clone()
            .unwrap_or_else(|| self.saved_groups())
    }

    /// The engine always installs the always-on group, when it has one.
    fn with_always_on(&self, mut set: BTreeSet<String>) -> BTreeSet<String> {
        if self.model.group(ALWAYS_ON).is_some() {
            set.insert(ALWAYS_ON.to_string());
        }
        set
    }

    /// Ids in groups.tsv order (the order of the engine's group events).
    pub fn in_group_order(&self, set: &BTreeSet<String>) -> Vec<String> {
        self.model
            .group_order()
            .into_iter()
            .filter(|g| set.contains(*g))
            .map(str::to_string)
            .collect()
    }

    fn set_pending(&mut self, set: BTreeSet<String>) {
        let set = self.with_always_on(set);
        self.pending_groups = (set != self.saved_groups()).then_some(set);
    }

    /// Groups the pending selection adds and removes, in groups.tsv order.
    pub fn pending_diff(&self) -> (Vec<String>, Vec<String>) {
        let Some(pending) = &self.pending_groups else {
            return (vec![], vec![]);
        };
        let saved = self.saved_groups();
        let added = self.in_group_order(&pending.difference(&saved).cloned().collect());
        let removed = self.in_group_order(&saved.difference(pending).cloned().collect());
        (added, removed)
    }

    /// Profiles whose groups (plus the always-on group) are the current selection.
    pub fn profile_matches(&self, id: &str) -> bool {
        let current = self.effective_groups();
        !current.is_empty()
            && self.model.profiles.iter().any(|p| {
                p.id == id && self.with_always_on(p.groups.iter().cloned().collect()) == current
            })
    }

    fn toggle_group(&mut self) {
        let Some(id) = self
            .model
            .group_order()
            .get(self.grp_sel)
            .map(|s| s.to_string())
        else {
            return;
        };
        if id == ALWAYS_ON {
            self.say(format!("{ALWAYS_ON} is always installed."), Status::Skip);
            return;
        }
        let mut set = self.effective_groups();
        if !set.remove(&id) {
            set.insert(id);
        }
        self.set_pending(set);
    }

    fn apply_profile(&mut self) {
        match self.model.profiles.get(self.prof_sel) {
            Some(p) => {
                let set = p.groups.iter().cloned().collect();
                self.set_pending(set);
            }
            None => self.say("No profiles to apply.", Status::Skip),
        }
    }

    fn save_groups(&mut self) {
        match &self.pending_groups {
            Some(set) => {
                let groups = self.in_group_order(set);
                self.confirm(Action::SetGroups { groups });
            }
            None => self.say(
                "No unsaved changes: toggle groups with Space or apply a profile with p.",
                Status::Skip,
            ),
        }
    }

    fn profiles_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => self.prof_focus = ProfilesFocus::Groups,
            KeyCode::Right | KeyCode::Char('l') => self.prof_focus = ProfilesFocus::Profiles,
            KeyCode::Char(' ') => match self.prof_focus {
                ProfilesFocus::Groups => self.toggle_group(),
                ProfilesFocus::Profiles => self.apply_profile(),
            },
            KeyCode::Char('p') => self.apply_profile(),
            KeyCode::Enter | KeyCode::Char('s') => self.save_groups(),
            KeyCode::Char('R') => self.confirm(Action::Reset),
            _ => {}
        }
    }

    // --- actions ------------------------------------------------------------

    fn say(&mut self, text: impl Into<String>, status: Status) {
        self.toast = Some(Toast {
            text: text.into(),
            status,
        });
    }

    fn confirm(&mut self, action: Action) {
        self.dialog = Some(Dialog {
            action,
            preview: None,
        });
    }

    fn on_item(&mut self, make: impl Fn(&Item) -> Action) {
        let action = match self.package_rows().get(self.pkg_sel) {
            Some(Row::Item(item)) => Some(make(item)),
            _ => None,
        };
        match action {
            Some(a) => self.confirm(a),
            None => self.say(
                "Select a package first (open a group with Enter).",
                Status::Warn,
            ),
        }
    }

    /// `i`: the selected package, or every missing package in the selected group.
    fn install(&mut self) {
        let ids: Vec<String> = match self.package_rows().get(self.pkg_sel) {
            Some(Row::Item(item)) => vec![item.id.clone()],
            Some(Row::Group { id, .. }) => self
                .model
                .items
                .iter()
                .filter(|i| i.group == *id && i.state == ItemState::Missing)
                .map(|i| i.id.clone())
                .collect(),
            None => vec![],
        };
        if ids.is_empty() {
            self.say("Nothing missing here to install.", Status::Skip);
        } else {
            self.confirm(Action::Install { ids });
        }
    }

    fn capture_add(&mut self) {
        match self.model.untracked.get(self.cap_sel) {
            Some(u) => {
                let id = if u.id.is_empty() {
                    format!("{}:{}", u.kind, u.name)
                } else {
                    u.id.clone()
                };
                self.pick(PickFor::Capture { id }, None);
            }
            None => self.say("Nothing to add.", Status::Skip),
        }
    }

    fn review(&mut self, choice: ReviewChoice) {
        let Some(d) = self.model.dotfiles.get(self.dot_sel) else {
            self.say("No dotfile selected.", Status::Skip);
            return;
        };
        if d.state != DotfileState::Differs {
            self.say(
                "Nothing to review: this file doesn't exist yet, and setup would write it.",
                Status::Skip,
            );
            return;
        }
        let file = d.path.clone();
        self.confirm(Action::Review { file, choice });
    }

    /// Open the group picker, leaving out the item's current group.
    fn pick(&mut self, purpose: PickFor, except: Option<&str>) {
        let options: Vec<String> = self
            .model
            .group_order()
            .into_iter()
            .filter(|g| Some(*g) != except)
            .map(str::to_string)
            .collect();
        if options.is_empty() {
            self.say("No other group to choose.", Status::Skip);
            return;
        }
        self.picker = Some(Picker {
            purpose,
            options,
            sel: 0,
        });
    }

    fn picker_key(&mut self, key: KeyEvent) {
        let Some(p) = &mut self.picker else { return };
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.picker = None,
            KeyCode::Up | KeyCode::Char('k') => p.sel = p.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                p.sel = (p.sel + 1).min(p.options.len().saturating_sub(1));
            }
            KeyCode::Enter => {
                let to = p.options[p.sel].clone();
                let action = match &p.purpose {
                    PickFor::Move { id } => Action::Move { id: id.clone(), to },
                    PickFor::Capture { id } => Action::CaptureAdd { id: id.clone(), to },
                };
                self.picker = None;
                self.confirm(action);
            }
            _ => {}
        }
    }

    /// A dry run for the open dialog is queued or running.
    pub fn previewing(&self) -> bool {
        match self.dialog.as_ref().and_then(|d| d.preview) {
            Some(None) => true,
            Some(Some(id)) => self.activity.get(id).is_some_and(|j| j.running()),
            None => false,
        }
    }

    fn dialog_key(&mut self, key: KeyEvent) {
        let previewing = self.previewing();
        let Some(d) = &mut self.dialog else { return };
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => self.dialog = None,
            KeyCode::Enter | KeyCode::Char('y') if !previewing => {
                let action = d.action.clone();
                self.dialog = None;
                self.enqueue_action(action, false);
            }
            KeyCode::Char('D') if !previewing && d.action.can_dry_run() => {
                d.preview = Some(None);
                let action = d.action.clone();
                self.enqueue_action(action, true);
            }
            _ => {}
        }
    }

    fn cancel(&mut self) {
        match (self.running, self.running_pid) {
            (Some((id, _)), Some(pid)) if self.cancelling.is_none() => {
                self.cancelling = Some(id);
                self.effects.push(Effect::Kill(pid));
                self.say("Cancelling…", Status::Warn);
            }
            (Some(_), None) => self.say("This job can't be cancelled from here.", Status::Skip),
            (Some(_), Some(_)) => {}
            (None, _) => self.say("Nothing is running.", Status::Skip),
        }
    }

    fn selected_log(&self) -> Option<String> {
        let i = self.activity_index()?;
        let log = &self.activity.jobs[i].log;
        (!log.is_empty()).then(|| log.clone())
    }

    fn copy_log(&mut self) {
        match self.selected_log() {
            Some(log) => {
                self.effects.push(Effect::Copy(log.clone()));
                let shown = self.tilde_str(&log);
                self.say(format!("Copied {shown}"), Status::Ok);
            }
            None => self.say("This job has no log yet.", Status::Skip),
        }
    }

    fn open_log(&mut self) {
        let log = [&self.model.check, &self.model.list]
            .iter()
            .find_map(|r| {
                r.done
                    .as_ref()
                    .map(|d| d.log.clone())
                    .or(r.hello.as_ref().map(|h| h.log.clone()))
            })
            .filter(|l| !l.is_empty());
        match log {
            Some(l) => self.effects.push(Effect::Open(l)),
            None => self.say("No log yet: run a check with c.", Status::Skip),
        }
    }
}
