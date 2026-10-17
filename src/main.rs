//! macsetup: a terminal dashboard for a macOS setup engine.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use macsetup::app::{App, Effect, FirstRun, Screen, Tab};
use macsetup::config::{self, FindError, Inputs, Source};
use macsetup::engine::{self, Envelope, Msg};
use macsetup::model::JobError;
use macsetup::model::JobKind;
use macsetup::theme::Theme;
use macsetup::ui;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

const HELP: &str = "\
Terminal dashboard for a macOS setup engine.

Usage: macsetup [--repo PATH]
       macsetup --version | --help

Options:
  --repo PATH   use the engine repo at PATH (overrides discovery)

The engine is found via --repo, then $MACSETUP_REPO, then `repo = \"…\"` in
~/.config/macsetup/config.toml, then `chezmoi source-path`.";

struct Args {
    repo: Option<String>,
    /// Dev flag: render each tab once to DIR/<tab>.txt instead of running.
    snapshot: Option<PathBuf>,
    size: (u16, u16),
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut args = Args {
        repo: None,
        snapshot: None,
        size: (100, 30),
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--version" | "-V" => {
                println!("macsetup {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--help" | "-h" => {
                println!("macsetup {}\n\n{HELP}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--repo" => args.repo = Some(value("--repo")?),
            "--snapshot" => args.snapshot = Some(PathBuf::from(value("--snapshot")?)),
            "--size" => {
                let v = value("--size")?;
                let (w, h) = v
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                    .ok_or(format!("--size wants WIDTHxHEIGHT, got {v:?}"))?;
                args.size = (w, h);
            }
            s if s.starts_with("--repo=") => args.repo = Some(s["--repo=".len()..].to_string()),
            other => return Err(format!("unknown argument '{other}' (try --help)")),
        }
    }
    Ok(Some(args))
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(a)) => a,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("macsetup: {e}");
            return ExitCode::from(2);
        }
    };
    let mut inputs = Inputs::from_env();
    inputs.flag_repo = args.repo.clone();

    if let Some(dir) = &args.snapshot {
        return match snapshot(&inputs, dir, args.size) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("macsetup: {e}");
                ExitCode::FAILURE
            }
        };
    }

    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        eprintln!("macsetup: needs an interactive terminal (stdin and stdout must be a TTY)");
        return ExitCode::FAILURE;
    }

    let mut app = App::new(Screen::Main, Theme::from_env(), inputs.home.clone());
    start(&mut app, &inputs);

    let (tx, rx) = mpsc::channel();
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app, &tx, &rx);
    ratatui::restore();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("macsetup: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Decide the first screen: connected, first run, or a fatal message.
fn start(app: &mut App, inputs: &Inputs) {
    match config::find_repo(inputs) {
        Ok((repo, source)) => connect(app, repo, source),
        Err(FindError::NotFound) => {
            let input = config::chezmoi_candidate(inputs)
                .map(|p| app.tilde(&p))
                .unwrap_or_default();
            match &inputs.config_file {
                Some(file) => {
                    app.screen = Screen::FirstRun(FirstRun {
                        input,
                        error: None,
                        config_file: file.clone(),
                    })
                }
                None => app.fatal("No setup engine", FindError::NotFound.to_string()),
            }
        }
        Err(FindError::Invalid(e)) => app.fatal("Engine not usable", e),
    }
}

/// Check the engine's API, then show the dashboard (or why it can't).
fn connect(app: &mut App, repo: PathBuf, source: Source) {
    match engine::api(&repo) {
        Ok((_, info)) if info.api == engine::API => app.connected(repo, source, info.api),
        Ok((_, info)) => {
            let shown = app.tilde(&repo);
            app.fatal(
                "Engine API mismatch",
                format!(
                    "The engine at {shown} speaks API {}, this dashboard speaks API {}. \
                     Update whichever is older: `brew upgrade macsetup` for the dashboard, \
                     or pull the engine repo.",
                    info.api,
                    engine::API
                ),
            )
        }
        Err(e) => app.fatal("Engine not usable", e),
    }
}

fn run(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    tx: &Sender<Envelope>,
    rx: &Receiver<Envelope>,
) -> std::io::Result<()> {
    let mut dirty = true;
    loop {
        if let Some(repo) = app.after_first_run.take() {
            connect(app, repo, Source::ConfigFile);
            dirty = true;
        }
        if let (Some((id, kind)), Some(repo)) = (app.take_next_job(), app.repo.clone()) {
            let job = app.running_job.clone().expect("a job was just started");
            if job.attached() {
                attached(terminal, app, &repo, id, kind, &job.args())?;
            } else {
                let pid = engine::spawn_args(&repo, &job.args(), kind, id, tx.clone());
                app.started(pid);
            }
            dirty = true;
        }
        for effect in std::mem::take(&mut app.effects) {
            do_effect(effect);
        }
        while let Ok(env) = rx.try_recv() {
            app.on_engine(env);
            dirty = true;
        }
        if dirty {
            terminal.draw(|f| ui::draw(f, app))?;
            dirty = false;
        }
        if app.quit {
            return Ok(());
        }
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    app.on_key(k);
                    dirty = true;
                }
                Event::Resize(..) => dirty = true,
                _ => {}
            }
        } else if app.running.is_some() {
            app.tick();
            dirty = true;
        }
    }
}

/// Hand the terminal to `setup` for an action that may prompt (a password,
/// a merge tool), then come back and replay what it reported.
fn attached(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    repo: &Path,
    id: u64,
    kind: JobKind,
    args: &[String],
) -> std::io::Result<()> {
    use std::io::Write;
    ratatui::restore();
    let events = std::env::temp_dir().join(format!(
        "macsetup-events-{}-{id}.ndjson",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&events);
    println!("\n$ setup {}\n", args.join(" "));
    let status = engine::run_attached(repo, args, &events);
    print!("\nPress any key to return to the dashboard");
    std::io::stdout().flush()?;
    ratatui::crossterm::terminal::enable_raw_mode()?;
    loop {
        if let Event::Key(k) = event::read()?
            && k.kind == KeyEventKind::Press
        {
            break;
        }
    }
    ratatui::crossterm::terminal::disable_raw_mode()?;
    *terminal = ratatui::init();
    terminal.clear()?;

    let (parsed, bad) = engine::read_events(&events);
    let _ = std::fs::remove_file(&events);
    let saw_done = parsed
        .iter()
        .any(|e| matches!(e, macsetup::events::Event::Done(_)));
    let send = |app: &mut App, msg| app.on_engine(Envelope { job: id, kind, msg });
    for e in parsed {
        send(app, Msg::Event(e));
    }
    for l in bad {
        send(app, Msg::BadLine(l));
    }
    let result = match status {
        Err(e) => Err(JobError::Spawn(e.to_string())),
        Ok(_) if saw_done => Ok(()),
        Ok(st) => Err(JobError::NoDone {
            exit: st.code(),
            stderr: String::new(),
        }),
    };
    send(app, Msg::Finished(result));
    Ok(())
}

fn do_effect(effect: Effect) {
    use std::process::{Command, Stdio};
    match effect {
        Effect::Kill(pid) => engine::cancel(pid),
        Effect::Open(path) => {
            let _ = Command::new("open")
                .arg(path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
        Effect::Copy(text) => {
            if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    use std::io::Write;
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
            }
        }
    }
}

/// Run `list`, `capture --list` and `check` to completion and write each tab's text render.
fn snapshot(inputs: &Inputs, dir: &Path, (w, h): (u16, u16)) -> Result<(), String> {
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = App::new(Screen::Main, Theme::plain(), inputs.home.clone());
    start(&mut app, inputs);
    if let Screen::Main = app.screen {
        app.queue.clear();
        let repo = app.repo.clone().expect("connected");
        for kind in [JobKind::List, JobKind::CaptureList, JobKind::Check] {
            if let Err(e) = engine::run_blocking(&repo, kind, &mut app.model) {
                app.last_error = Some(format!("{}: {e}", kind.label()));
            }
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for tab in Tab::ALL {
        app.tab = tab;
        let mut terminal = Terminal::new(TestBackend::new(w, h)).map_err(|e| e.to_string())?;
        terminal
            .draw(|f| ui::draw(f, &app))
            .map_err(|e| e.to_string())?;
        let path = dir.join(format!("{}.txt", tab.title().to_lowercase()));
        std::fs::write(&path, format!("{}\n", terminal.backend()))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if !matches!(app.screen, Screen::Main) {
            break; // first-run and fatal screens look the same on every tab
        }
    }
    Ok(())
}
