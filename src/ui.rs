//! Rendering: a pure function of [`App`].

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Cell, Clear, List, ListItem, ListState, Paragraph, Row as TableRow, Table, Wrap,
};

use crate::app::{App, FirstRun, Row, SPINNER, Screen, Tab};
use crate::events::{DotfileState, ItemState, meta_text};
use crate::model::JobKind;
use crate::theme::{dotfile_symbol, status_symbol};

pub const MIN_WIDTH: u16 = 60;
pub const MIN_HEIGHT: u16 = 15;
/// Below this width, detail panes move under their lists.
const SIDE_BY_SIDE: u16 = 90;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let msg = format!(
            "Terminal too small: {}×{} (needs {MIN_WIDTH}×{MIN_HEIGHT})",
            area.width, area.height
        );
        frame.render_widget(
            Paragraph::new(msg).wrap(Wrap { trim: true }).centered(),
            centered(area, area.width, 3),
        );
        return;
    }
    match &app.screen {
        Screen::FirstRun(fr) => draw_first_run(frame, app, fr, area),
        Screen::Fatal { title, body } => draw_fatal(frame, app, title, body, area),
        Screen::Main => draw_main(frame, app, area),
    }
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn block(title: &str) -> Block<'_> {
    Block::bordered().title(format!(" {title} "))
}

// --- first run and fatal screens ------------------------------------------

fn draw_first_run(frame: &mut Frame, app: &App, fr: &FirstRun, area: Rect) {
    let t = app.theme;
    let box_area = centered(area, 76, 16);
    let mut lines = vec![
        Line::from("macsetup drives a setup engine: a repo with an executable `setup`"),
        Line::from("at its root. None was found via --repo, $MACSETUP_REPO, config.toml"),
        Line::from("or chezmoi."),
        Line::from(""),
        Line::from("Type the path of the engine repo and press Enter to save it to"),
        Line::from(Span::styled(app.tilde(&fr.config_file), t.heading())),
        Line::from(""),
        Line::from(vec![
            Span::styled("Path: ", t.heading()),
            Span::raw(fr.input.clone()),
            Span::styled("▏", t.key()),
        ]),
        Line::from(""),
    ];
    if let Some(e) = &fr.error {
        lines.push(Line::from(Span::styled(e.clone(), t.error())));
    } else {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Enter", t.key()),
        Span::raw(" save and continue   "),
        Span::styled("Esc", t.key()),
        Span::raw(" quit"),
    ]));
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block("Where is your setup engine?")),
        box_area,
    );
}

fn draw_fatal(frame: &mut Frame, app: &App, title: &str, body: &str, area: Rect) {
    let t = app.theme;
    let box_area = centered(area, 76, 12);
    let text = Text::from(vec![
        Line::from(body.to_string()),
        Line::from(""),
        Line::from(vec![Span::styled("q", t.key()), Span::raw(" quit")]),
    ]);
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(block(title).border_style(t.error())),
        box_area,
    );
}

// --- main screen ------------------------------------------------------------

fn draw_main(frame: &mut Frame, app: &App, area: Rect) {
    let [tabs, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);
    draw_tabs(frame, app, tabs);
    match app.tab {
        Tab::Overview => draw_overview(frame, app, body),
        Tab::Packages => draw_packages(frame, app, body),
        Tab::Dotfiles => draw_dotfiles(frame, app, body),
    }
    draw_status(frame, app, status);
    if app.show_help {
        draw_help(frame, app, area);
    }
}

fn draw_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let mut spans = vec![Span::styled(" macsetup ", t.heading()), Span::raw(" ")];
    for (i, tab) in Tab::ALL.iter().enumerate() {
        let label = format!(" {} {} ", i + 1, tab.title());
        let style = if *tab == app.tab {
            t.tab_active()
        } else {
            Style::new()
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
    }
    let [left, right] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(9)]).areas(area);
    frame.render_widget(Line::from(spans), left);
    frame.render_widget(
        Line::from(vec![Span::styled("?", t.key()), Span::raw(" help ")]).right_aligned(),
        right,
    );
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let mut spans = Vec::new();
    if let Some(repo) = &app.repo {
        spans.push(Span::styled(" engine ", t.dim()));
        spans.push(Span::raw(app.tilde(repo)));
    }
    if let Some(api) = app.api {
        spans.push(Span::styled(format!("  API {api}"), t.dim()));
    }
    spans.push(Span::raw("  "));
    match app.running {
        Some((_, kind)) => {
            spans.push(Span::styled(SPINNER[app.spinner], t.key()));
            spans.push(Span::raw(format!(" {}…", kind.label())));
        }
        None => match &app.model.check.done {
            Some(d) => spans.push(Span::styled(
                format!(
                    "last check: {} ok · {} warnings · {} failed",
                    d.ok, d.warn, d.fail
                ),
                t.dim(),
            )),
            None => spans.push(Span::styled("idle", t.dim())),
        },
    }
    if let Some(err) = &app.last_error {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(format!("✗ {err}"), t.error()));
    }
    frame.render_widget(Line::from(spans), area);
}

/// A placeholder message for loading, empty and error states.
fn notice(frame: &mut Frame, area: Rect, title: &str, text: Line<'_>) {
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .block(block(title)),
        area,
    );
}

/// Loading / error / empty for anything fed by `setup list`.
fn list_state_notice(app: &App) -> Option<Line<'static>> {
    let t = app.theme;
    if !app.model.groups.is_empty() || !app.model.items.is_empty() {
        return None;
    }
    if app.is_running(JobKind::List) || app.queue.contains(&JobKind::List) {
        return Some(Line::from("Loading the package list…"));
    }
    if let Some(e) = &app.model.list.error {
        return Some(Line::from(vec![
            Span::styled(format!("Couldn't load the package list: {e}. "), t.error()),
            Span::raw("Press r to retry."),
        ]));
    }
    Some(Line::from(
        "The engine reported no groups or packages. Press r to reload.",
    ))
}

// --- Overview ---------------------------------------------------------------

fn draw_overview(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let status_rows = app.model.status_results().count() as u16;
    let [groups_area, status_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length((status_rows + 3).clamp(4, 10)),
    ])
    .areas(area);

    if let Some(line) = list_state_notice(app) {
        notice(frame, groups_area, "Groups", line);
    } else {
        let counts = app.model.counts();
        let check_pending = app.is_running(JobKind::Check) || app.queue.contains(&JobKind::Check);
        let header = TableRow::new(vec![
            Cell::from("Group"),
            Cell::from("✓ installed"),
            Cell::from("✗ missing"),
            Cell::from("◆ outside"),
            Cell::from("! wrong app"),
            Cell::from("? unknown"),
            Cell::from("check"),
        ])
        .style(t.heading());
        let rows = app.model.group_order().into_iter().map(|id| {
            let c = counts.get(id).copied().unwrap_or_default();
            let title = app
                .model
                .group(id)
                .map(|g| {
                    if g.title.is_empty() {
                        id
                    } else {
                        g.title.as_str()
                    }
                })
                .unwrap_or(id);
            let num = |n: usize, s: ItemState| {
                if n == 0 {
                    Cell::from(Span::styled("·", t.dim()))
                } else {
                    Cell::from(Span::styled(n.to_string(), t.item_state(s)))
                }
            };
            let check = match app.model.group_result(id) {
                Some(r) => Cell::from(Span::styled(
                    format!("{} {}", status_symbol(r.status), r.status.label()),
                    t.status(r.status),
                )),
                None if check_pending => Cell::from(Span::styled("…", t.dim())),
                None => Cell::from(Span::styled("–", t.dim())),
            };
            TableRow::new(vec![
                Cell::from(title.to_string()),
                num(c.installed, ItemState::Installed),
                num(c.missing, ItemState::Missing),
                num(c.outside, ItemState::Outside),
                num(c.mismatch, ItemState::Mismatch),
                num(c.unknown, ItemState::Unknown),
                check,
            ])
        });
        let total = app.model.total_counts();
        let title = format!(
            "Groups · {} packages, {} missing",
            total.total(),
            total.missing
        );
        let table = Table::new(
            rows,
            [
                Constraint::Fill(1),
                Constraint::Length(12),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(12),
                Constraint::Length(10),
                Constraint::Length(10),
            ],
        )
        .header(header)
        .block(block(&title));
        frame.render_widget(table, groups_area);
    }

    // Repos, secrets, GitHub identity, and where the log is.
    let mut lines: Vec<Line> = Vec::new();
    if let Some(e) = &app.model.check.error {
        lines.push(Line::from(Span::styled(
            format!("Check failed: {e}. Press c to retry."),
            t.error(),
        )));
    } else if app.model.check.done.is_none() && app.model.results.is_empty() {
        let msg = if app.is_running(JobKind::Check) || app.queue.contains(&JobKind::Check) {
            "Checking…"
        } else {
            "Not checked yet. Press c to run a check."
        };
        lines.push(Line::from(Span::styled(msg, t.dim())));
    }
    for r in app.model.status_results() {
        let detail = r.detail.lines().next().unwrap_or("");
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", status_symbol(r.status)), t.status(r.status)),
            Span::raw(r.item.clone()),
            Span::styled(
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", app.tilde_str(detail))
                },
                t.dim(),
            ),
        ]));
    }
    let log = app
        .model
        .check
        .done
        .as_ref()
        .map(|d| d.log.as_str())
        .or(app.model.check.hello.as_ref().map(|h| h.log.as_str()))
        .filter(|l| !l.is_empty());
    if let Some(log) = log {
        lines.push(Line::from(vec![
            Span::styled("log ", t.dim()),
            Span::raw(app.tilde_str(log)),
        ]));
    }
    frame.render_widget(Paragraph::new(lines).block(block("Status")), status_area);
}

// --- Packages ---------------------------------------------------------------

fn split_detail(area: Rect) -> (Rect, Rect) {
    if area.width >= SIDE_BY_SIDE {
        let [a, b] = Layout::horizontal([Constraint::Fill(3), Constraint::Fill(2)]).areas(area);
        (a, b)
    } else {
        let [a, b] = Layout::vertical([Constraint::Fill(1), Constraint::Length(9)]).areas(area);
        (a, b)
    }
}

fn draw_packages(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    if let Some(line) = list_state_notice(app) {
        notice(frame, area, "Packages", line);
        return;
    }
    let (list_area, detail_area) = split_detail(area);

    let mut title = String::from("Packages");
    if let Some(s) = app.state_filter {
        title.push_str(&format!(" · only {}", s.label()));
    }
    if app.editing_filter || !app.text_filter.is_empty() {
        title.push_str(&format!(
            " · /{}{}",
            app.text_filter,
            if app.editing_filter { "▏" } else { "" }
        ));
    }

    let rows = app.package_rows();
    if rows.is_empty() {
        notice(
            frame,
            list_area,
            &title,
            Line::from("Nothing matches the filter. Esc clears it."),
        );
    } else {
        let items: Vec<ListItem> = rows
            .iter()
            .map(|row| match row {
                Row::Group {
                    id,
                    expanded,
                    counts,
                } => {
                    let name = app
                        .model
                        .group(id)
                        .map(|g| {
                            if g.title.is_empty() {
                                *id
                            } else {
                                g.title.as_str()
                            }
                        })
                        .unwrap_or(id);
                    let mut spans = vec![
                        Span::raw(if *expanded { "▾ " } else { "▸ " }),
                        Span::styled(name.to_string(), t.heading()),
                        Span::styled(format!("  {}", counts.total()), t.dim()),
                    ];
                    for s in [ItemState::Missing, ItemState::Mismatch] {
                        let n = counts.get(s);
                        if n > 0 {
                            spans.push(Span::styled(
                                format!("  {} {n}", s.symbol()),
                                t.item_state(s),
                            ));
                        }
                    }
                    ListItem::new(Line::from(spans))
                }
                Row::Item(item) => ListItem::new(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(item.state.symbol(), t.item_state(item.state)),
                    Span::raw(" "),
                    Span::raw(item.name.clone()),
                    Span::styled(format!("  {}", item.kind), t.dim()),
                ])),
            })
            .collect();
        let list = List::new(items)
            .block(block(&title))
            .highlight_style(t.selected());
        let mut state = ListState::default().with_selected(Some(app.pkg_sel));
        frame.render_stateful_widget(list, list_area, &mut state);
    }

    let detail: Vec<Line> = match rows.get(app.pkg_sel) {
        Some(Row::Item(item)) => {
            let mut lines = vec![
                Line::from(Span::styled(item.name.clone(), t.heading())),
                Line::from(Span::styled(
                    format!("{} {}", item.state.symbol(), item.state.label()),
                    t.item_state(item.state),
                )),
                Line::from(Span::styled(
                    format!(
                        "{} · {}",
                        item.kind,
                        app.model
                            .group(&item.group)
                            .map(|g| g.title.as_str())
                            .filter(|s| !s.is_empty())
                            .unwrap_or(&item.group)
                    ),
                    t.dim(),
                )),
                Line::from(""),
            ];
            if !item.desc.is_empty() {
                lines.push(Line::from(item.desc.clone()));
                lines.push(Line::from(""));
            }
            for (k, v) in &item.meta {
                lines.push(Line::from(vec![
                    Span::styled(format!("{k}: "), t.dim()),
                    Span::raw(meta_text(v)),
                ]));
            }
            lines.push(Line::from(Span::styled(item.id.clone(), t.dim())));
            lines
        }
        Some(Row::Group { id, counts, .. }) => {
            let g = app.model.group(id);
            let mut lines = vec![Line::from(Span::styled(
                g.map(|g| g.title.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| id.to_string()),
                t.heading(),
            ))];
            if let Some(g) = g.filter(|g| !g.desc.is_empty()) {
                lines.push(Line::from(Span::styled(g.desc.clone(), t.dim())));
            }
            lines.push(Line::from(""));
            for s in ItemState::ALL {
                let n = counts.get(s);
                if n > 0 {
                    lines.push(Line::from(vec![
                        Span::styled(format!("{} {n} ", s.symbol()), t.item_state(s)),
                        Span::raw(s.label()),
                    ]));
                }
            }
            if let Some(r) = app.model.group_result(id) {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("check: ", t.dim()),
                    Span::styled(
                        format!("{} {}", status_symbol(r.status), r.status.label()),
                        t.status(r.status),
                    ),
                ]));
            }
            lines
        }
        None => vec![],
    };
    frame.render_widget(
        Paragraph::new(detail)
            .wrap(Wrap { trim: false })
            .block(block("Details")),
        detail_area,
    );
}

// --- Dotfiles ---------------------------------------------------------------

fn draw_dotfiles(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let checking = app.is_running(JobKind::Check) || app.queue.contains(&JobKind::Check);
    if app.model.dotfiles.is_empty() {
        let line = if let Some(e) = &app.model.check.error {
            Line::from(vec![
                Span::styled(format!("Check failed: {e}. "), t.error()),
                Span::raw("Press c to retry."),
            ])
        } else if checking {
            Line::from("Checking dotfiles…")
        } else if let Some(r) = app.model.stage_problems("dotfiles").next() {
            // e.g. chezmoi isn't set up yet: say what the engine said, not "all match"
            Line::from(vec![
                Span::styled(format!("{} ", status_symbol(r.status)), t.status(r.status)),
                Span::raw(format!("{}: {}", r.item, r.detail)),
            ])
        } else if app.model.check.done.is_some() {
            Line::from("All dotfiles match the repo.")
        } else {
            Line::from("Not checked yet. Press c to run a check.")
        };
        notice(frame, area, "Dotfiles", line);
        return;
    }
    let (list_area, detail_area) = split_detail(area);
    let items: Vec<ListItem> = app
        .model
        .dotfiles
        .iter()
        .map(|d| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{} {:<8}", dotfile_symbol(d.state), d.state.label()),
                    t.dotfile(d.state),
                ),
                Span::raw(d.path.clone()),
            ]))
        })
        .collect();
    let title = format!("Dotfiles · {}", app.model.dotfiles.len());
    let list = List::new(items)
        .block(block(&title))
        .highlight_style(t.selected());
    let mut state = ListState::default().with_selected(Some(app.dot_sel));
    frame.render_stateful_widget(list, list_area, &mut state);

    let detail: Vec<Line> = match app.model.dotfiles.get(app.dot_sel) {
        Some(d) => {
            let mut lines = vec![
                Line::from(Span::styled(d.path.clone(), t.heading())),
                Line::from(""),
            ];
            match d.state {
                DotfileState::Differs => {
                    lines.push(Line::from(
                        "Your file differs from the repo. setup never overwrites it.",
                    ));
                    lines.push(Line::from(""));
                    lines.push(Line::from("`setup review` offers:"));
                    for choice in [
                        "keep yours",
                        "take the repo's (backs yours up)",
                        "merge them",
                        "decide later",
                    ] {
                        lines.push(Line::from(format!("  • {choice}")));
                    }
                }
                DotfileState::Pending => {
                    lines.push(Line::from(
                        "This file doesn't exist yet; setup would write it from the repo.",
                    ));
                }
                DotfileState::Unknown => {
                    lines.push(Line::from(
                        "The engine reported a state this version doesn't know.",
                    ));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Actions arrive in a later version.",
                t.dim(),
            )));
            lines
        }
        None => vec![],
    };
    frame.render_widget(
        Paragraph::new(detail)
            .wrap(Wrap { trim: false })
            .block(block("Details")),
        detail_area,
    );
}

// --- help -------------------------------------------------------------------

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let key = |k: &'static str, what: &'static str| {
        Line::from(vec![
            Span::styled(format!("{k:<14}"), t.key()),
            Span::raw(what),
        ])
    };
    let mut lines = vec![
        key("1 2 3  Tab", "switch tab"),
        key("↑ ↓  j k", "move"),
        key("Enter  Space", "open or close a group"),
        key("→ ←  l h", "open / close a group"),
        key("f", "filter by state (Packages)"),
        key("/", "filter by text (Enter keeps it)"),
        key("Esc", "clear filters, else quit"),
        key("r", "reload the package list"),
        key("c", "run a check"),
        key("q", "quit"),
        Line::from(""),
    ];
    // The state legend, wrapped over two lines.
    for (i, chunk) in ItemState::ALL.chunks(3).enumerate() {
        let mut legend = vec![Span::styled(
            if i == 0 {
                "States        "
            } else {
                "              "
            },
            t.heading(),
        )];
        for s in chunk {
            legend.push(Span::styled(
                format!("{} {}   ", s.symbol(), s.label()),
                t.item_state(*s),
            ));
        }
        lines.push(Line::from(legend));
    }
    let popup = centered(area, 72, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block("Keys · any key closes")),
        popup,
    );
}
