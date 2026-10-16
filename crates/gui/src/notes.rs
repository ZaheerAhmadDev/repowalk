//! Notes panel (v2 F11): pin local reading notes to a file, symbol, or line.
//! OWNER: TEXT-4.
//!
//! Everything here is local: notes live in `<repo>/.repowalk/notes.json` (already
//! gitignored by `**/.repowalk/`), written atomically the same way
//! `repowalk_core::store` writes its cache. There is no network, no LLM, and no
//! editing of source. [`load`] tolerates a missing or corrupt file by returning an
//! empty store and reporting the problem — it never panics.
//!
//! Honesty rules this panel follows:
//! - no repo / no analysis is stated, never rendered as an empty list;
//! - an empty store invites the first note instead of showing a blank pane;
//! - a mutation that fails to persist is rolled back and the error is surfaced
//!   (we never claim a note was saved when the write failed);
//! - a note whose target is gone from the index is marked `stale`, not dropped.
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::{App, Selection};
use crate::sidebar::{data_row, Meta};
use crate::theme;
use repowalk_core::model::Index;

/// Current on-disk schema for `notes.json`.
const NOTES_VERSION: u32 = 1;

/// Where a note points: always a file, optionally a symbol id and a line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Target {
    pub file: String,
    pub symbol: Option<String>,
    pub line: Option<usize>,
}

/// One local note. `created` / `updated` are RFC3339 UTC strings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Note {
    pub id: String,
    pub target: Target,
    pub text: String,
    pub created: String,
    pub updated: String,
}

/// The whole `notes.json` document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteStore {
    pub version: u32,
    pub notes: Vec<Note>,
}

impl Default for NoteStore {
    fn default() -> Self {
        Self {
            version: NOTES_VERSION,
            notes: Vec::new(),
        }
    }
}

/// `<root>/.repowalk/notes.json`
pub fn notes_path(root: &Path) -> PathBuf {
    root.join(".repowalk").join("notes.json")
}

/// Load the store; a missing or corrupt file yields an empty store, never a panic.
///
/// This plain form is part of the module's store API (and what a reader reaches
/// for first); the panel itself uses [`load_with_status`] so it can also explain
/// *why* a store came back empty.
#[allow(dead_code)]
pub fn load(root: &Path) -> NoteStore {
    load_with_status(root).0
}

/// Like [`load`], but also says *why* the store is empty when a file existed
/// (unreadable / corrupt / version mismatch), so the panel can explain itself
/// instead of pretending the notes were never there.
pub fn load_with_status(root: &Path) -> (NoteStore, Option<String>) {
    let path = notes_path(root);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (NoteStore::default(), None),
        Err(e) => {
            return (
                NoteStore::default(),
                Some(format!("could not read {}: {e}", path.display())),
            )
        }
    };
    match json::decode(&bytes) {
        Ok(store) => {
            let warn = (store.version != NOTES_VERSION).then(|| {
                format!(
                    "{} has version {} (expected {}) — loaded best-effort",
                    path.display(),
                    store.version,
                    NOTES_VERSION
                )
            });
            (store, warn)
        }
        Err(e) => (
            NoteStore::default(),
            Some(format!(
                "{} is not valid notes JSON ({e}) — starting empty",
                path.display()
            )),
        ),
    }
}

/// Atomic write (tmp + rename). Creates `.repowalk/`. Never leaves a partial file.
pub fn save(root: &Path, store: &NoteStore) -> anyhow::Result<()> {
    let dir = root.join(".repowalk");
    std::fs::create_dir_all(&dir)?;
    let dest = notes_path(root);
    let tmp = dir.join("notes.json.tmp");
    std::fs::write(&tmp, json::encode(store).into_bytes())?;
    std::fs::rename(&tmp, &dest)?;
    Ok(())
}

// ── Timestamps (hand-rolled RFC3339 UTC; no `chrono`/`time` dependency) ─────

fn rfc3339(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant's `civil_from_days`: days since the Unix epoch → (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `YYYY-MM-DD` for display, from a stored RFC3339 string. Never panics on a
/// short or non-ASCII value.
fn short_date(stamp: &str) -> String {
    match stamp.get(..10) {
        Some(s) => s.to_string(),
        None if stamp.is_empty() => "—".to_string(),
        None => stamp.to_string(),
    }
}

// ── Store mutations (pure; `at` is injectable so tests stay deterministic) ──

fn new_id(at: SystemTime) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = at
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("n{nanos:x}-{n:x}")
}

/// Add a note; returns its new id.
pub fn add(store: &mut NoteStore, target: Target, text: &str, at: SystemTime) -> String {
    let id = new_id(at);
    let stamp = rfc3339(at);
    store.notes.push(Note {
        id: id.clone(),
        target,
        text: text.to_string(),
        created: stamp.clone(),
        updated: stamp,
    });
    id
}

/// Update a note's text + `updated`; returns whether the id existed.
pub fn update_text(store: &mut NoteStore, id: &str, text: &str, at: SystemTime) -> bool {
    match store.notes.iter_mut().find(|n| n.id == id) {
        Some(n) => {
            n.text = text.to_string();
            n.updated = rfc3339(at);
            true
        }
        None => false,
    }
}

/// Delete a note; returns whether the id existed.
pub fn remove(store: &mut NoteStore, id: &str) -> bool {
    let before = store.notes.len();
    store.notes.retain(|n| n.id != id);
    store.notes.len() != before
}

/// A note is stale when its file — or, if set, its symbol id — is gone from the
/// index. Stale notes are marked, never silently dropped.
pub fn is_stale(note: &Note, index: &Index) -> bool {
    if !index.files.iter().any(|f| f.path == note.target.file) {
        return true;
    }
    match &note.target.symbol {
        Some(id) => !index.symbols.iter().any(|s| &s.id == id),
        None => false,
    }
}

// ── View helpers ────────────────────────────────────────────────────────────

fn target_label(t: &Target) -> String {
    match (t.line, t.symbol.is_some()) {
        (Some(l), true) => format!("{}:{} · symbol", t.file, l),
        (Some(l), false) => format!("{}:{}", t.file, l),
        (None, true) => format!("{} · symbol", t.file),
        (None, false) => t.file.clone(),
    }
}

fn matches(note: &Note, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    note.text.to_lowercase().contains(query)
        || note.target.file.to_lowercase().contains(query)
        || note
            .target
            .symbol
            .as_deref()
            .is_some_and(|s| s.to_lowercase().contains(query))
}

/// The file / symbol / line a new note should attach to, from the current
/// selection (falling back to the open file when there is no selection).
fn selection_target(app: &App) -> Option<Target> {
    match &app.selection {
        Some(Selection::Symbol(id)) => {
            if let Some(idx) = &app.index {
                if let Some(s) = idx.symbols.iter().find(|s| &s.id == id) {
                    return Some(Target {
                        file: s.file.clone(),
                        symbol: Some(id.clone()),
                        line: Some(s.start_line),
                    });
                }
            }
            // No index entry: derive the file from the `path:line` id shape.
            let (file, line) = id
                .rsplit_once(':')
                .map_or((id.clone(), None), |(f, l)| (f.to_string(), l.parse().ok()));
            Some(Target {
                file,
                symbol: Some(id.clone()),
                line,
            })
        }
        Some(Selection::File(p)) => Some(Target {
            file: p.clone(),
            symbol: None,
            line: app.jump_line,
        }),
        None => app.open_file.clone().map(|p| Target {
            file: p,
            symbol: None,
            line: app.jump_line,
        }),
    }
}

/// Jump to a note's target; when the symbol still exists, select it so every
/// pane follows, otherwise open the file at its line.
fn jump(app: &mut App, note: &Note) {
    let live_symbol = note.target.symbol.clone().filter(|id| {
        app.index
            .as_ref()
            .is_some_and(|i| i.symbols.iter().any(|s| &s.id == id))
    });
    if let Some(id) = live_symbol {
        app.select(Selection::Symbol(id));
    } else {
        app.open_file_with(note.target.file.clone(), note.target.line);
        app.selection = Some(Selection::File(note.target.file.clone()));
    }
}

// ── Panel state (egui memory; the shared `App` must not grow fields) ────────

#[derive(Clone, Default)]
struct UiState {
    loaded_repo: Option<PathBuf>,
    store: NoteStore,
    query: String,
    draft: String,
    editing: Option<String>,
    edit_buf: String,
    error: Option<String>,
    notice: Option<String>,
}

enum Action {
    Add,
    Jump(String),
    StartEdit(String),
    CancelEdit,
    SaveEdit,
    Delete(String),
}

/// A note prepared for rendering (owned, so the list can borrow `state` mutably).
struct RowView {
    id: String,
    file: String,
    text: String,
    target: String,
    created: String,
    stale: bool,
}

fn build_rows(store: &NoteStore, query: &str, index: Option<&Index>) -> Vec<RowView> {
    let q = query.trim().to_lowercase();
    store
        .notes
        .iter()
        .filter(|n| matches(n, &q))
        .map(|n| RowView {
            id: n.id.clone(),
            file: n.target.file.clone(),
            text: n.text.clone(),
            target: target_label(&n.target),
            created: short_date(&n.created),
            stale: index.is_some_and(|i| is_stale(n, i)),
        })
        .collect()
}

/// Group rows by file, preserving first-appearance order.
fn group_rows(rows: Vec<RowView>) -> Vec<(String, Vec<RowView>)> {
    let mut groups: Vec<(String, Vec<RowView>)> = Vec::new();
    for row in rows {
        match groups.iter_mut().find(|(f, _)| *f == row.file) {
            Some((_, v)) => v.push(row),
            None => {
                let file = row.file.clone();
                groups.push((file, vec![row]));
            }
        }
    }
    groups
}

// ── Panel ───────────────────────────────────────────────────────────────────

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let id = egui::Id::new("notes-panel-state");
    let mut state = ui
        .ctx()
        .data_mut(|d| d.get_temp::<UiState>(id))
        .unwrap_or_default();

    let Some(repo) = app.repo.clone() else {
        placeholder(ui, "no repository loaded — open a repo to keep notes");
        ui.ctx().data_mut(|d| d.insert_temp(id, state));
        return;
    };
    if app.index.is_none() {
        placeholder(ui, "no analysis yet — notes unlock once a repo is learned");
        ui.ctx().data_mut(|d| d.insert_temp(id, state));
        return;
    }

    // (Re)load from disk when the repo changes.
    if state.loaded_repo.as_deref() != Some(repo.as_path()) {
        let (store, warning) = load_with_status(&repo);
        state.store = store;
        state.loaded_repo = Some(repo.clone());
        state.query.clear();
        state.draft.clear();
        state.editing = None;
        state.edit_buf.clear();
        state.notice = None;
        // A corrupt read is surfaced, not hidden behind an empty list.
        state.error = warning;
    }

    let target = selection_target(app);

    theme::section_header(ui, "NOTES");
    count_line(ui, &state, app.index.as_deref());
    if let Some(err) = &state.error {
        ui.colored_label(theme::DANGER, format!("✕ {err}"));
    } else if let Some(note) = &state.notice {
        ui.colored_label(theme::SUCCESS, note);
    }
    search_box(ui, &mut state);
    theme::separator(ui);
    let mut action = add_box(ui, &mut state, target.as_ref());
    theme::separator(ui);

    let rows = build_rows(&state.store, &state.query, app.index.as_deref());
    let open_file = app.open_file.clone();
    let jump_line = app.jump_line;

    if state.store.notes.is_empty() {
        empty_invite(ui);
    } else if rows.is_empty() {
        muted(ui, &format!("no notes match “{}”", state.query.trim()));
    } else {
        let groups = group_rows(rows);
        egui::ScrollArea::vertical()
            .id_salt("notes-list")
            .show(ui, |ui| {
                for (file, notes) in &groups {
                    file_header(ui, file, notes.len());
                    for row in notes {
                        let selected = state.editing.as_deref() == Some(row.id.as_str())
                            || (open_file.as_deref() == Some(row.file.as_str())
                                && jump_line
                                    .is_some_and(|l| row.target.ends_with(&format!(":{l}"))));
                        let meta = if row.stale {
                            Meta::tinted("stale", theme::WARNING)
                        } else {
                            Meta::muted(&row.target)
                        };
                        let resp = data_row(
                            ui,
                            selected,
                            ("note", row.id.as_str()),
                            egui::RichText::new(row.text.as_str())
                                .size(theme::FONT_SMALL)
                                .color(theme::TEXT_SECONDARY),
                            Some(meta),
                        );
                        if resp.clicked() {
                            action = Some(Action::Jump(row.id.clone()));
                        }
                        resp.on_hover_text(format!("{} · {}", row.target, row.created));
                        note_meta_line(ui, row, &mut state, &mut action);
                    }
                }
            });
    }

    match action {
        Some(Action::Add) => {
            if let Some(t) = target {
                let text = state.draft.trim().to_string();
                if !text.is_empty() {
                    let before = state.store.clone();
                    let draft = state.draft.clone();
                    add(&mut state.store, t, &text, SystemTime::now());
                    match save(&repo, &state.store) {
                        Ok(()) => {
                            state.draft.clear();
                            state.error = None;
                            state.notice = Some("note saved".to_string());
                        }
                        Err(e) => {
                            // Roll back so the list never shows a note that is
                            // not on disk; keep the draft so nothing is lost.
                            state.store = before;
                            state.draft = draft;
                            let msg = format!("could not save notes: {e:#}");
                            state.error = Some(msg.clone());
                            app.error = Some(msg);
                        }
                    }
                }
            }
        }
        Some(Action::Jump(note_id)) => {
            if let Some(note) = state.store.notes.iter().find(|n| n.id == note_id) {
                jump(app, note);
            }
        }
        Some(Action::StartEdit(note_id)) => {
            if let Some(note) = state.store.notes.iter().find(|n| n.id == note_id) {
                state.edit_buf = note.text.clone();
                state.editing = Some(note_id);
            }
        }
        Some(Action::CancelEdit) => {
            state.editing = None;
            state.edit_buf.clear();
        }
        Some(Action::SaveEdit) => {
            if let Some(note_id) = state.editing.clone() {
                let before = state.store.clone();
                if update_text(
                    &mut state.store,
                    &note_id,
                    state.edit_buf.trim(),
                    SystemTime::now(),
                ) {
                    match save(&repo, &state.store) {
                        Ok(()) => {
                            state.editing = None;
                            state.edit_buf.clear();
                            state.error = None;
                            state.notice = Some("note updated".to_string());
                        }
                        Err(e) => {
                            state.store = before;
                            let msg = format!("could not save notes: {e:#}");
                            state.error = Some(msg.clone());
                            app.error = Some(msg);
                        }
                    }
                } else {
                    state.editing = None;
                    state.edit_buf.clear();
                }
            }
        }
        Some(Action::Delete(note_id)) => {
            let before = state.store.clone();
            if remove(&mut state.store, &note_id) {
                match save(&repo, &state.store) {
                    Ok(()) => {
                        if state.editing.as_deref() == Some(note_id.as_str()) {
                            state.editing = None;
                            state.edit_buf.clear();
                        }
                        state.error = None;
                        state.notice = Some("note deleted".to_string());
                    }
                    Err(e) => {
                        state.store = before;
                        let msg = format!("could not save notes: {e:#}");
                        state.error = Some(msg.clone());
                        app.error = Some(msg);
                    }
                }
            }
        }
        None => {}
    }

    ui.ctx().data_mut(|d| d.insert_temp(id, state));
}

fn count_line(ui: &mut egui::Ui, state: &UiState, index: Option<&Index>) {
    let total = state.store.notes.len();
    let stale = index.map_or(0, |i| {
        state.store.notes.iter().filter(|n| is_stale(n, i)).count()
    });
    let mut text = format!("{total} note(s)");
    if stale > 0 {
        text.push_str(&format!(" · {stale} stale"));
    }
    let color = if stale > 0 {
        theme::WARNING
    } else {
        theme::TEXT_MUTED
    };
    ui.label(
        egui::RichText::new(text)
            .size(theme::FONT_SMALL)
            .color(color),
    );
}

fn search_box(ui: &mut egui::Ui, state: &mut UiState) {
    ui.add(
        egui::TextEdit::singleline(&mut state.query)
            .hint_text("filter notes…")
            .desired_width(f32::INFINITY),
    );
}

fn add_box(ui: &mut egui::Ui, state: &mut UiState, target: Option<&Target>) -> Option<Action> {
    match target {
        Some(t) => ui.label(
            egui::RichText::new(format!("new note for {}", target_label(t)))
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_SECONDARY),
        ),
        None => ui.label(
            egui::RichText::new("select a file or symbol in Text mode to attach a note")
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        ),
    };
    ui.add(
        egui::TextEdit::multiline(&mut state.draft)
            .hint_text("write a note…")
            .desired_rows(2)
            .desired_width(f32::INFINITY),
    );
    let can_add = target.is_some() && !state.draft.trim().is_empty();
    ui.add_enabled(can_add, egui::Button::new("Add note"))
        .clicked()
        .then_some(Action::Add)
}

fn file_header(ui: &mut egui::Ui, file: &str, count: usize) {
    ui.add_space(theme::SPACE_2);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
        ui.add(
            egui::Label::new(
                egui::RichText::new(file)
                    .size(theme::FONT_SMALL)
                    .strong()
                    .color(theme::TEXT_SECONDARY),
            )
            .truncate(),
        );
        ui.label(
            egui::RichText::new(format!("({count})"))
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
    });
}

/// Date + stale marker + edit/delete for one note; renders the inline editor
/// when this row is being edited.
fn note_meta_line(
    ui: &mut egui::Ui,
    row: &RowView,
    state: &mut UiState,
    action: &mut Option<Action>,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(row.created.as_str())
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
        if row.stale {
            ui.label(
                egui::RichText::new("· target no longer in index")
                    .size(theme::FONT_SMALL)
                    .color(theme::WARNING),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("delete").clicked() {
                *action = Some(Action::Delete(row.id.clone()));
            }
            if ui.small_button("edit").clicked() {
                *action = Some(Action::StartEdit(row.id.clone()));
            }
        });
    });
    if state.editing.as_deref() == Some(row.id.as_str()) {
        ui.add(
            egui::TextEdit::multiline(&mut state.edit_buf)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                *action = Some(Action::SaveEdit);
            }
            if ui.button("Cancel").clicked() {
                *action = Some(Action::CancelEdit);
            }
        });
    }
}

/// Invite the first note instead of showing a blank pane.
fn empty_invite(ui: &mut egui::Ui) {
    ui.add_space(theme::SPACE_6);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("No notes yet")
                .size(theme::FONT_BODY)
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(theme::SPACE_2);
        ui.label(
            egui::RichText::new(
                "Pick a file or symbol in Text mode, then write the first note above.",
            )
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
        );
    });
}

fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
    );
}

fn placeholder(ui: &mut egui::Ui, text: &str) {
    ui.add_space(theme::SPACE_6);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(text)
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_SECONDARY),
        );
    });
}

// ── Minimal JSON codec (no `serde_json` dependency) ─────────────────────────
//
// `notes.json` is a small, flat document, and `repowalk-gui` does not depend on
// `serde_json`, so the store is encoded/decoded here. The encoder is exact; the
// decoder is deliberately forgiving (unknown keys ignored, missing fields
// defaulted) and returns `Err` for anything malformed — `load_with_status`
// turns that into an empty store plus a visible warning, never a panic.
mod json {
    use super::{Note, NoteStore, Target};

    /// Serialize the store as pretty, valid JSON.
    pub fn encode(store: &NoteStore) -> String {
        let mut out = String::from("{\n  \"version\": ");
        out.push_str(&store.version.to_string());
        out.push_str(",\n  \"notes\": [");
        for (i, n) in store.notes.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("\n    {\n      \"id\": ");
            out.push_str(&quote(&n.id));
            out.push_str(",\n      \"target\": {\"file\": ");
            out.push_str(&quote(&n.target.file));
            if let Some(sym) = &n.target.symbol {
                out.push_str(", \"symbol\": ");
                out.push_str(&quote(sym));
            }
            if let Some(line) = n.target.line {
                out.push_str(", \"line\": ");
                out.push_str(&line.to_string());
            }
            out.push_str("},\n      \"text\": ");
            out.push_str(&quote(&n.text));
            out.push_str(",\n      \"created\": ");
            out.push_str(&quote(&n.created));
            out.push_str(",\n      \"updated\": ");
            out.push_str(&quote(&n.updated));
            out.push_str("\n    }");
        }
        if !store.notes.is_empty() {
            out.push_str("\n  ");
        }
        out.push_str("]\n}\n");
        out
    }

    /// Parse bytes into a store; any malformed input is an `Err`, never a panic.
    pub fn decode(bytes: &[u8]) -> Result<NoteStore, String> {
        let text = std::str::from_utf8(bytes).map_err(|e| format!("invalid UTF-8: {e}"))?;
        let mut p = Parser::new(text);
        let value = p.parse_value()?;
        p.skip_ws();
        if !p.at_end() {
            return Err("trailing data after JSON value".to_string());
        }
        store_from_value(value)
    }

    fn quote(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    enum Value {
        Null,
        Bool,
        Num(f64),
        Str(String),
        Arr(Vec<Value>),
        Obj(Vec<(String, Value)>),
    }

    fn store_from_value(v: Value) -> Result<NoteStore, String> {
        let Value::Obj(fields) = v else {
            return Err("root is not an object".to_string());
        };
        let mut store = NoteStore::default();
        for (k, val) in fields {
            match k.as_str() {
                "version" => {
                    if let Value::Num(n) = val {
                        store.version = n.max(0.0) as u32;
                    }
                }
                "notes" => {
                    if let Value::Arr(items) = val {
                        store.notes = items.into_iter().map(note_from_value).collect();
                    }
                }
                _ => {}
            }
        }
        Ok(store)
    }

    fn note_from_value(v: Value) -> Note {
        let mut note = Note::default();
        let Value::Obj(fields) = v else {
            return note;
        };
        for (k, val) in fields {
            match k.as_str() {
                "id" => {
                    if let Value::Str(s) = val {
                        note.id = s;
                    }
                }
                "text" => {
                    if let Value::Str(s) = val {
                        note.text = s;
                    }
                }
                "created" => {
                    if let Value::Str(s) = val {
                        note.created = s;
                    }
                }
                "updated" => {
                    if let Value::Str(s) = val {
                        note.updated = s;
                    }
                }
                "target" => note.target = target_from_value(val),
                _ => {}
            }
        }
        note
    }

    fn target_from_value(v: Value) -> Target {
        let mut t = Target::default();
        let Value::Obj(fields) = v else {
            return t;
        };
        for (k, val) in fields {
            match k.as_str() {
                "file" => {
                    if let Value::Str(s) = val {
                        t.file = s;
                    }
                }
                "symbol" => {
                    if let Value::Str(s) = val {
                        t.symbol = Some(s);
                    }
                }
                "line" => {
                    if let Value::Num(n) = val {
                        t.line = Some(n.max(0.0) as usize);
                    }
                }
                _ => {}
            }
        }
        t
    }

    struct Parser {
        chars: Vec<char>,
        i: usize,
    }

    impl Parser {
        fn new(s: &str) -> Self {
            Self {
                chars: s.chars().collect(),
                i: 0,
            }
        }

        fn at_end(&self) -> bool {
            self.i >= self.chars.len()
        }

        fn peek(&self) -> Option<char> {
            self.chars.get(self.i).copied()
        }

        fn skip_ws(&mut self) {
            while matches!(self.peek(), Some(c) if c.is_whitespace()) {
                self.i += 1;
            }
        }

        fn parse_value(&mut self) -> Result<Value, String> {
            self.skip_ws();
            match self.peek() {
                Some('{') => self.parse_object(),
                Some('[') => self.parse_array(),
                Some('"') => self.parse_string().map(Value::Str),
                Some('t') => self.literal("true", Value::Bool),
                Some('f') => self.literal("false", Value::Bool),
                Some('n') => self.literal("null", Value::Null),
                Some(c) if c == '-' || c.is_ascii_digit() => self.parse_number().map(Value::Num),
                Some(c) => Err(format!("unexpected character '{c}'")),
                None => Err("unexpected end of input".to_string()),
            }
        }

        fn literal(&mut self, word: &str, v: Value) -> Result<Value, String> {
            for expected in word.chars() {
                if self.peek() == Some(expected) {
                    self.i += 1;
                } else {
                    return Err(format!("expected '{word}'"));
                }
            }
            Ok(v)
        }

        fn parse_object(&mut self) -> Result<Value, String> {
            self.i += 1; // '{'
            let mut fields = Vec::new();
            self.skip_ws();
            if self.peek() == Some('}') {
                self.i += 1;
                return Ok(Value::Obj(fields));
            }
            loop {
                self.skip_ws();
                let key = self.parse_string()?;
                self.skip_ws();
                if self.peek() != Some(':') {
                    return Err("expected ':'".to_string());
                }
                self.i += 1;
                let val = self.parse_value()?;
                fields.push((key, val));
                self.skip_ws();
                match self.peek() {
                    Some(',') => self.i += 1,
                    Some('}') => {
                        self.i += 1;
                        break;
                    }
                    _ => return Err("expected ',' or '}'".to_string()),
                }
            }
            Ok(Value::Obj(fields))
        }

        fn parse_array(&mut self) -> Result<Value, String> {
            self.i += 1; // '['
            let mut items = Vec::new();
            self.skip_ws();
            if self.peek() == Some(']') {
                self.i += 1;
                return Ok(Value::Arr(items));
            }
            loop {
                items.push(self.parse_value()?);
                self.skip_ws();
                match self.peek() {
                    Some(',') => self.i += 1,
                    Some(']') => {
                        self.i += 1;
                        break;
                    }
                    _ => return Err("expected ',' or ']'".to_string()),
                }
            }
            Ok(Value::Arr(items))
        }

        fn parse_string(&mut self) -> Result<String, String> {
            if self.peek() != Some('"') {
                return Err("expected string".to_string());
            }
            self.i += 1;
            let mut out = String::new();
            loop {
                match self.peek() {
                    None => return Err("unterminated string".to_string()),
                    Some('"') => {
                        self.i += 1;
                        return Ok(out);
                    }
                    Some('\\') => {
                        self.i += 1;
                        match self.peek() {
                            Some('"') => out.push('"'),
                            Some('\\') => out.push('\\'),
                            Some('/') => out.push('/'),
                            Some('n') => out.push('\n'),
                            Some('r') => out.push('\r'),
                            Some('t') => out.push('\t'),
                            Some('b') => out.push('\u{0008}'),
                            Some('f') => out.push('\u{000C}'),
                            Some('u') => {
                                self.i += 1;
                                let mut code = 0u32;
                                for _ in 0..4 {
                                    let d = self
                                        .peek()
                                        .and_then(|c| c.to_digit(16))
                                        .ok_or("bad \\u escape")?;
                                    code = code * 16 + d;
                                    self.i += 1;
                                }
                                out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                                continue;
                            }
                            _ => return Err("bad escape".to_string()),
                        }
                        self.i += 1;
                    }
                    Some(c) => {
                        out.push(c);
                        self.i += 1;
                    }
                }
            }
        }

        fn parse_number(&mut self) -> Result<f64, String> {
            let start = self.i;
            while matches!(self.peek(), Some(c)
                if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E')
            {
                self.i += 1;
            }
            let s: String = self.chars[start..self.i].iter().collect();
            s.parse::<f64>().map_err(|_| format!("bad number '{s}'"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::model::{FileEntry, Symbol, SymbolKind};
    use std::time::Duration;

    fn temp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("rw-notes-test-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn index_with(files: &[&str], symbols: &[&str]) -> Index {
        let mut idx = Index::new("/repo");
        for f in files {
            idx.files.push(FileEntry {
                path: (*f).into(),
                lang: "rust".into(),
                lines: 10,
                mtime: 0,
                is_entrypoint: false,
            });
        }
        for id in symbols {
            let file = id.rsplit_once(':').map_or(*id, |(f, _)| f).to_string();
            idx.symbols.push(Symbol {
                id: (*id).into(),
                name: "sym".into(),
                kind: SymbolKind::Function,
                signature: String::new(),
                doc: String::new(),
                start_line: 1,
                end_line: 3,
                container: None,
                file,
            });
        }
        idx
    }

    fn target(file: &str, line: usize) -> Target {
        Target {
            file: file.into(),
            symbol: None,
            line: Some(line),
        }
    }

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn roundtrip_save_load_in_temp_dir() {
        let root = temp_root("roundtrip");
        // Missing file → empty, no warning, no panic.
        let (missing, warn) = load_with_status(&root);
        assert!(missing.notes.is_empty());
        assert!(warn.is_none());

        let mut store = NoteStore::default();
        let id = add(
            &mut store,
            Target {
                file: "src/a.rs".into(),
                symbol: Some("src/a.rs:3".into()),
                line: Some(3),
            },
            "reminder",
            at(1_700_000_000),
        );
        save(&root, &store).unwrap();

        let loaded = load(&root);
        assert_eq!(loaded, store);
        assert_eq!(loaded.notes.len(), 1);
        assert_eq!(loaded.notes[0].id, id);
        assert_eq!(loaded.notes[0].created, "2023-11-14T22:13:20Z");
        assert_eq!(loaded.notes[0].updated, "2023-11-14T22:13:20Z");
        // atomic write left no temp file behind
        assert!(!root.join(".repowalk").join("notes.json.tmp").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn json_codec_escapes_and_tolerates_junk() {
        let root = temp_root("escape");
        let mut store = NoteStore::default();
        add(
            &mut store,
            Target {
                file: "src/a\"b.rs".into(),
                symbol: None,
                line: None,
            },
            "line1\nline2\t\"quoted\" \\ slash",
            at(42),
        );
        save(&root, &store).unwrap();
        assert_eq!(load(&root), store);

        // Malformed input is an Err, never a panic.
        assert!(json::decode(b"").is_err());
        assert!(json::decode(b"{").is_err());
        assert!(json::decode(b"[1,2]").is_err());
        assert!(json::decode(b"{\"notes\": [{").is_err());
        assert!(json::decode(b"{} {}").is_err());

        // Unknown keys and missing fields are tolerated.
        let ok = json::decode(br#"{"version":1,"junk":true,"notes":[]}"#).unwrap();
        assert!(ok.notes.is_empty());
        let partial = json::decode(br#"{"notes":[{"text":"hi"}]}"#).unwrap();
        assert_eq!(partial.notes.len(), 1);
        assert_eq!(partial.notes[0].text, "hi");
        assert!(partial.notes[0].target.file.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_file_loads_empty_without_panicking() {
        let root = temp_root("corrupt");
        std::fs::create_dir_all(root.join(".repowalk")).unwrap();
        std::fs::write(notes_path(&root), b"{ this is not json").unwrap();

        let (store, warn) = load_with_status(&root);
        assert!(store.notes.is_empty());
        assert!(warn.is_some(), "corruption must be reported, not hidden");
        // The plain loader stays tolerant too.
        assert!(load(&root).notes.is_empty());

        // A valid file with a wrong version still loads, with a notice.
        std::fs::write(notes_path(&root), br#"{"version":99,"notes":[]}"#).unwrap();
        let (store, warn) = load_with_status(&root);
        assert_eq!(store.version, 99);
        assert!(warn.is_some());
        assert!(load(&root).notes.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn stale_target_detection_marks_but_keeps() {
        let idx = index_with(&["src/a.rs"], &["src/a.rs:3"]);

        let live = Note {
            id: "1".into(),
            target: Target {
                file: "src/a.rs".into(),
                symbol: Some("src/a.rs:3".into()),
                line: Some(3),
            },
            text: "still valid".into(),
            created: String::new(),
            updated: String::new(),
        };
        assert!(!is_stale(&live, &idx));

        // File present but the symbol is gone.
        let mut symbol_gone = live.clone();
        symbol_gone.target.symbol = Some("src/a.rs:99".into());
        symbol_gone.target.line = Some(99);
        assert!(is_stale(&symbol_gone, &idx));

        // File gone entirely.
        let mut file_gone = live.clone();
        file_gone.target = target("src/gone.rs", 1);
        assert!(is_stale(&file_gone, &idx));

        // A file-only note on a present file is live.
        let file_only = Note {
            target: target("src/a.rs", 1),
            ..live.clone()
        };
        assert!(!is_stale(&file_only, &idx));
    }

    #[test]
    fn add_edit_delete_logic() {
        let mut store = NoteStore::default();
        let id = add(&mut store, target("f.rs", 1), "first", at(1_000));
        assert_eq!(store.notes.len(), 1);
        assert_eq!(store.notes[0].text, "first");
        assert_eq!(store.notes[0].updated, rfc3339(at(1_000)));

        // Edit bumps `updated`, keeps `created`.
        assert!(update_text(&mut store, &id, "second", at(1_060)));
        assert_eq!(store.notes[0].text, "second");
        assert_eq!(store.notes[0].created, rfc3339(at(1_000)));
        assert_eq!(store.notes[0].updated, rfc3339(at(1_060)));
        assert!(!update_text(&mut store, "missing", "x", at(1_060)));

        // Delete removes once; a second delete is a no-op.
        assert!(remove(&mut store, &id));
        assert!(store.notes.is_empty());
        assert!(!remove(&mut store, &id));

        // Ids are unique even within the same second.
        let a = add(&mut store, target("f.rs", 1), "a", at(2_000));
        let b = add(&mut store, target("f.rs", 1), "b", at(2_000));
        assert_ne!(a, b);
    }

    #[test]
    fn timestamp_helpers_match_rfc3339() {
        assert_eq!(rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(at(1_700_000_000)), "2023-11-14T22:13:20Z");
        assert_eq!(short_date("2023-11-14T22:13:20Z"), "2023-11-14");
        assert_eq!(short_date(""), "—");
        // A short/odd value never panics.
        assert_eq!(short_date("nope"), "nope");
    }

    #[test]
    fn filter_and_group_rows() {
        let mut store = NoteStore::default();
        add(&mut store, target("b.rs", 1), "alpha", at(1));
        add(&mut store, target("a.rs", 2), "beta", at(2));
        add(&mut store, target("b.rs", 5), "gamma", at(3));
        let idx = index_with(&["a.rs", "b.rs"], &[]);

        let groups = group_rows(build_rows(&store, "", Some(&idx)));
        assert_eq!(groups.len(), 2);
        // First-appearance order: b.rs was seen first.
        assert_eq!(groups[0].0, "b.rs");
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].0, "a.rs");
        assert_eq!(groups[1].1.len(), 1);

        let filtered = build_rows(&store, "alpha", Some(&idx));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].text, "alpha");
        assert_eq!(filtered[0].target, "b.rs:1");

        // Query matches file paths too.
        assert_eq!(build_rows(&store, "a.rs", Some(&idx)).len(), 1);
        // No match → empty (panel shows the filtered-empty state).
        assert!(build_rows(&store, "zzz", Some(&idx)).is_empty());
    }
}
