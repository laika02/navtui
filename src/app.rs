use std::collections::HashSet;
use std::io;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, KeyboardEnhancementFlags, ModifierKeyCode, MouseButton, MouseEvent,
    MouseEventKind, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use rand::seq::SliceRandom;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{
    Block, Borders, List, ListItem, ListState, Paragraph,
    block::{Position, Title},
};
use ratatui::{Frame, Terminal};

use crate::cache as disk_cache;
use crate::config::KeybindsConfig;
use crate::library::LibraryCache;
use crate::model::{Album, Song};
use crate::playback::PlaybackEngine;
use crate::state::{Action, BrowserState, Outcome, Tab};
use crate::subsonic::SubsonicClient;

pub fn run(
    client: SubsonicClient,
    cache: LibraryCache,
    expand_on_search_collapse: bool,
    show_identity_label: bool,
    keybinds: KeybindsConfig,
) -> Result<LibraryCache> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    execute!(
        stdout,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES,
        )
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new(
        client,
        cache,
        expand_on_search_collapse,
        show_identity_label,
        keybinds,
    )
    .context("failed to initialize app state")?;
    let loop_result = app.run_loop(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        PopKeyboardEnhancementFlags
    )?;
    terminal.show_cursor()?;
    match loop_result {
        Ok(()) => Ok(app.into_cache()),
        Err(err) => Err(err),
    }
}

struct App {
    client: SubsonicClient,
    user_server_label: String,
    show_identity_label: bool,
    keybinds: KeyBindings,
    cache: LibraryCache,
    browser: BrowserState,
    queue: Vec<Song>,
    queue_index: Option<usize>,
    queue_nav_index: Option<usize>,
    queue_reorder_index: Option<usize>,
    queue_reorder_snapshot: Option<QueueReorderSnapshot>,
    player: PlaybackEngine,
    status: String,
    should_quit: bool,
    input_mode: InputMode,
    failed_retry_song_id: Option<String>,
    expand_on_search_collapse: bool,
    volume_percent: u8,
    playback_position_seconds: f64,
    playback_anchor_instant: Option<Instant>,
    volume_slider_hitbox: Option<VolumeSliderHitbox>,
    timeline_slider_hitbox: Option<TimelineSliderHitbox>,
    main_hitbox: Option<Rect>,
    queue_hitbox: Option<Rect>,
    queue_scroll_offset: usize,
    queue_visible_rows: usize,
    queue_follow_index: Option<usize>,
    interaction_mode: InteractionMode,
    library_warmup: Option<LibraryWarmupWorker>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrackEndAction {
    None,
    RetryCurrent,
    PauseOnFailure,
    AdvanceTo(usize),
    QueueComplete,
}

enum InputMode {
    Normal,
    Search { buffer: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InteractionMode {
    Browser,
    Queue,
}

struct QueueReorderSnapshot {
    queue: Vec<Song>,
    original_index: usize,
}

struct LibraryWarmupWorker {
    rx: Receiver<LibraryWarmupEvent>,
}

enum LibraryWarmupEvent {
    ArtistAlbums {
        artist_id: String,
        albums: Vec<Album>,
        done: usize,
        total: usize,
    },
    AlbumSongs {
        album_id: String,
        songs: Vec<Song>,
        done: usize,
        total: usize,
    },
    Done,
    Failed(String),
}

#[derive(Clone, Copy, Debug)]
struct VolumeSliderHitbox {
    x: u16,
    y: u16,
    width: u16,
}

#[derive(Clone, Copy, Debug)]
struct TimelineSliderHitbox {
    x: u16,
    y: u16,
    width: u16,
}

struct KeyBindings {
    queue_mode_toggle: BindingSet,
    quit: BindingSet,
    escape: BindingSet,
    global_reset: BindingSet,
    volume_down: BindingSet,
    volume_up: BindingSet,
    seek_back: BindingSet,
    seek_forward: BindingSet,
    search: BindingSet,
    tab_artists: BindingSet,
    tab_albums: BindingSet,
    tab_songs: BindingSet,
    tab_cycle: BindingSet,
    nav_up: BindingSet,
    nav_down: BindingSet,
    nav_left: BindingSet,
    activate: BindingSet,
    enqueue: BindingSet,
    play_next: BindingSet,
    play_pause: BindingSet,
    clear_queue: BindingSet,
    hard_refresh: BindingSet,
    shuffle: BindingSet,
    queue_back: BindingSet,
    queue_forward: BindingSet,
    queue_remove: BindingSet,
    queue_reorder_toggle: BindingSet,
    search_backspace: BindingSet,
}

#[derive(Clone)]
struct BindingSet {
    bindings: Vec<KeyBinding>,
}

#[derive(Clone, Copy)]
struct KeyBinding {
    code: KeyBindingCode,
    modifiers: KeyModifiers,
}

#[derive(Clone, Copy)]
enum KeyBindingCode {
    Key(KeyCode),
    ShiftModifierEvent,
}

impl KeyBindings {
    fn from_config(cfg: &KeybindsConfig) -> Result<Self> {
        Ok(Self {
            queue_mode_toggle: BindingSet::parse(
                &cfg.queue_mode_toggle,
                "keybinds.queue_mode_toggle",
            )?,
            quit: BindingSet::parse(&cfg.quit, "keybinds.quit")?,
            escape: BindingSet::parse(&cfg.escape, "keybinds.escape")?,
            global_reset: BindingSet::parse(&cfg.global_reset, "keybinds.global_reset")?,
            volume_down: BindingSet::parse(&cfg.volume_down, "keybinds.volume_down")?,
            volume_up: BindingSet::parse(&cfg.volume_up, "keybinds.volume_up")?,
            seek_back: BindingSet::parse(&cfg.seek_back, "keybinds.seek_back")?,
            seek_forward: BindingSet::parse(&cfg.seek_forward, "keybinds.seek_forward")?,
            search: BindingSet::parse(&cfg.search, "keybinds.search")?,
            tab_artists: BindingSet::parse(&cfg.tab_artists, "keybinds.tab_artists")?,
            tab_albums: BindingSet::parse(&cfg.tab_albums, "keybinds.tab_albums")?,
            tab_songs: BindingSet::parse(&cfg.tab_songs, "keybinds.tab_songs")?,
            tab_cycle: BindingSet::parse(&cfg.tab_cycle, "keybinds.tab_cycle")?,
            nav_up: BindingSet::parse(&cfg.nav_up, "keybinds.nav_up")?,
            nav_down: BindingSet::parse(&cfg.nav_down, "keybinds.nav_down")?,
            nav_left: BindingSet::parse(&cfg.nav_left, "keybinds.nav_left")?,
            activate: BindingSet::parse(&cfg.activate, "keybinds.activate")?,
            enqueue: BindingSet::parse(&cfg.enqueue, "keybinds.enqueue")?,
            play_next: BindingSet::parse(&cfg.play_next, "keybinds.play_next")?,
            play_pause: BindingSet::parse(&cfg.play_pause, "keybinds.play_pause")?,
            clear_queue: BindingSet::parse(&cfg.clear_queue, "keybinds.clear_queue")?,
            hard_refresh: BindingSet::parse(&cfg.hard_refresh, "keybinds.hard_refresh")?,
            shuffle: BindingSet::parse(&cfg.shuffle, "keybinds.shuffle")?,
            queue_back: BindingSet::parse(&cfg.queue_back, "keybinds.queue_back")?,
            queue_forward: BindingSet::parse(&cfg.queue_forward, "keybinds.queue_forward")?,
            queue_remove: BindingSet::parse(&cfg.queue_remove, "keybinds.queue_remove")?,
            queue_reorder_toggle: BindingSet::parse(
                &cfg.queue_reorder_toggle,
                "keybinds.queue_reorder_toggle",
            )?,
            search_backspace: BindingSet::parse(
                &cfg.search_backspace,
                "keybinds.search_backspace",
            )?,
        })
    }
}

impl BindingSet {
    fn parse(specs: &[String], name: &str) -> Result<Self> {
        let mut bindings = Vec::with_capacity(specs.len());
        for spec in specs {
            bindings.push(
                parse_keybinding_spec(spec)
                    .with_context(|| format!("invalid binding in {name}: {spec}"))?,
            );
        }
        Ok(Self { bindings })
    }

    fn matches(&self, key: KeyEvent) -> bool {
        self.bindings.iter().any(|binding| binding.matches(key))
    }
}

impl KeyBinding {
    fn matches(&self, key: KeyEvent) -> bool {
        match self.code {
            KeyBindingCode::ShiftModifierEvent => is_shift_modifier_key_event(key),
            KeyBindingCode::Key(expected) => {
                if expected == KeyCode::BackTab {
                    return key.code == KeyCode::BackTab;
                }
                if key.modifiers != self.modifiers {
                    return false;
                }
                match (expected, key.code) {
                    (KeyCode::Char(expected_char), KeyCode::Char(actual_char)) => {
                        if self.modifiers.contains(KeyModifiers::SHIFT)
                            && expected_char.is_ascii_alphabetic()
                        {
                            actual_char.eq_ignore_ascii_case(&expected_char)
                        } else {
                            actual_char == expected_char
                        }
                    }
                    _ => expected == key.code,
                }
            }
        }
    }
}

fn parse_keybinding_spec(raw: &str) -> Result<KeyBinding> {
    let spec = raw.trim();
    if spec.is_empty() {
        bail!("binding cannot be empty");
    }

    if spec.eq_ignore_ascii_case("shift") {
        return Ok(KeyBinding {
            code: KeyBindingCode::ShiftModifierEvent,
            modifiers: KeyModifiers::empty(),
        });
    }

    let mut modifiers = KeyModifiers::empty();
    let mut key_token: Option<&str> = None;

    for token in spec
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let normalized = token.to_ascii_lowercase();
        match normalized.as_str() {
            "shift" => modifiers |= KeyModifiers::SHIFT,
            "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
            "alt" => modifiers |= KeyModifiers::ALT,
            "super" | "meta" => modifiers |= KeyModifiers::SUPER,
            _ => {
                if key_token.is_some() {
                    bail!("only one non-modifier key is allowed");
                }
                key_token = Some(token);
            }
        }
    }

    let token = key_token.ok_or_else(|| anyhow!("missing key token"))?;
    let (code, inferred_shift) = parse_keycode_token(token)?;
    if inferred_shift && !modifiers.contains(KeyModifiers::SHIFT) {
        modifiers |= KeyModifiers::SHIFT;
    }

    Ok(KeyBinding {
        code: KeyBindingCode::Key(code),
        modifiers,
    })
}

fn parse_keycode_token(token: &str) -> Result<(KeyCode, bool)> {
    let normalized = token.to_ascii_lowercase();
    let code = match normalized.as_str() {
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "backspace" => KeyCode::Backspace,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdown" | "pgdn" => KeyCode::PageDown,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "space" => KeyCode::Char(' '),
        "plus" => KeyCode::Char('+'),
        _ => {
            if let Some(num) = normalized.strip_prefix('f')
                && let Ok(index) = num.parse::<u8>()
                && (1..=24).contains(&index)
            {
                return Ok((KeyCode::F(index), false));
            }
            let mut chars = token.chars();
            let Some(ch) = chars.next() else {
                bail!("empty key token");
            };
            if chars.next().is_some() {
                bail!("unknown key token '{token}'");
            }
            if ch.is_ascii_uppercase() {
                return Ok((KeyCode::Char(ch.to_ascii_lowercase()), true));
            }
            return Ok((KeyCode::Char(ch), false));
        }
    };
    Ok((code, false))
}

fn is_shift_modifier_key_event(key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Modifier(ModifierKeyCode::LeftShift)
        | KeyCode::Modifier(ModifierKeyCode::RightShift)
        | KeyCode::Modifier(ModifierKeyCode::IsoLevel3Shift)
        | KeyCode::Modifier(ModifierKeyCode::IsoLevel5Shift) => key.kind == KeyEventKind::Press,
        _ => false,
    }
}

impl App {
    fn new(
        client: SubsonicClient,
        cache: LibraryCache,
        expand_on_search_collapse: bool,
        show_identity_label: bool,
        keybinds_cfg: KeybindsConfig,
    ) -> Result<Self> {
        let artists = cache.artists().to_vec();
        let status = format!("Loaded {} artists", artists.len());
        let user_server_label = user_server_label(&client);
        let keybinds =
            KeyBindings::from_config(&keybinds_cfg).context("failed to parse keybinds config")?;
        let mut app = Self {
            client,
            user_server_label,
            show_identity_label,
            keybinds,
            cache,
            browser: BrowserState::new(artists),
            queue: Vec::new(),
            queue_index: None,
            queue_nav_index: None,
            queue_reorder_index: None,
            queue_reorder_snapshot: None,
            player: PlaybackEngine::new(),
            status,
            should_quit: false,
            input_mode: InputMode::Normal,
            failed_retry_song_id: None,
            expand_on_search_collapse,
            volume_percent: 70,
            playback_position_seconds: 0.0,
            playback_anchor_instant: None,
            volume_slider_hitbox: None,
            timeline_slider_hitbox: None,
            main_hitbox: None,
            queue_hitbox: None,
            queue_scroll_offset: 0,
            queue_visible_rows: 0,
            queue_follow_index: None,
            interaction_mode: InteractionMode::Browser,
            library_warmup: None,
        };
        app.start_background_library_warmup("startup");
        Ok(app)
    }

    fn into_cache(self) -> LibraryCache {
        self.cache
    }

    fn run_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        while !self.should_quit {
            if let Some(exit_status) = self.player.poll_finished()? {
                self.advance_after_track_end(exit_status);
            }
            self.poll_background_library_warmup();
            self.sync_reported_volume();
            terminal.draw(|frame| self.draw(frame))?;

            if event::poll(Duration::from_millis(120))? {
                match event::read()? {
                    Event::Key(key)
                        if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                    {
                        self.on_key(key)
                    }
                    Event::Mouse(mouse) => self.on_mouse(mouse),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn sync_reported_volume(&mut self) {
        if let Some(applied) = self.player.take_reported_volume_update() {
            self.volume_percent = applied.clamp(0, 100);
        }
    }

    fn start_background_library_warmup(&mut self, reason: &str) {
        if self.cache.has_all_songs_loaded() {
            self.library_warmup = None;
            return;
        }

        let loaded_artist_ids = self.cache.loaded_artist_ids();
        let loaded_album_ids = self.cache.loaded_album_ids();
        let artist_ids: Vec<String> = self
            .cache
            .artists()
            .iter()
            .map(|artist| artist.id.clone())
            .collect();

        let (tx, rx) = mpsc::channel();
        let client = self.client.clone();
        thread::spawn(move || {
            warm_library_in_background(client, artist_ids, loaded_artist_ids, loaded_album_ids, tx);
        });
        self.library_warmup = Some(LibraryWarmupWorker { rx });

        if self.status.is_empty() {
            self.status = format!("Library warmup started ({reason})");
        } else {
            self.status = format!("{} | warming cache in background", self.status);
        }
    }

    fn poll_background_library_warmup(&mut self) {
        loop {
            let message = {
                let Some(worker) = self.library_warmup.as_ref() else {
                    return;
                };
                match worker.rx.try_recv() {
                    Ok(message) => Some(message),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => {
                        self.library_warmup = None;
                        if self.status.contains("warming cache in background") {
                            self.status = "Library warmup stopped".to_string();
                        }
                        return;
                    }
                }
            };

            let Some(message) = message else {
                return;
            };
            match message {
                LibraryWarmupEvent::ArtistAlbums {
                    artist_id,
                    albums,
                    done,
                    total,
                } => {
                    self.cache.upsert_albums_for_artist(artist_id, albums);
                    if matches!(self.browser.active_tab(), Tab::Albums | Tab::Songs)
                        && (done == total || done % 8 == 0)
                    {
                        self.browser.refresh_active_tab_loaded(&self.cache);
                    }
                }
                LibraryWarmupEvent::AlbumSongs {
                    album_id,
                    songs,
                    done,
                    total,
                } => {
                    self.cache.upsert_songs_for_album(album_id, songs);
                    if self.browser.active_tab() == Tab::Songs && (done == total || done % 8 == 0) {
                        self.browser.refresh_active_tab_loaded(&self.cache);
                    }
                }
                LibraryWarmupEvent::Done => {
                    self.library_warmup = None;
                    self.browser.refresh_active_tab_loaded(&self.cache);
                    if self.status.contains("warming cache in background") {
                        self.status = "Library warmup complete".to_string();
                    }
                    if let Err(err) = disk_cache::save_library_snapshot(
                        self.client.server_url(),
                        self.client.username(),
                        &self.cache.snapshot(),
                    ) {
                        self.status = format!("Library warmup complete; cache save warning: {err}");
                    }
                    return;
                }
                LibraryWarmupEvent::Failed(err) => {
                    self.library_warmup = None;
                    self.status = format!("Library warmup failed: {err}");
                    return;
                }
            }
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Repeat && !self.repeat_allowed_for(key) {
            return;
        }

        if self.keybinds.queue_mode_toggle.matches(key) {
            self.toggle_interaction_mode();
            return;
        }

        if self.keybinds.quit.matches(key) {
            self.should_quit = true;
            return;
        }

        if self.keybinds.global_reset.matches(key) {
            self.reset_to_artists_home();
            return;
        }

        if self.keybinds.escape.matches(key) {
            if self.interaction_mode == InteractionMode::Queue {
                if self.queue_reorder_index.is_some() {
                    self.cancel_queue_reorder_mode();
                    return;
                }
                self.toggle_interaction_mode();
                return;
            }
            self.handle_escape();
            return;
        }

        if self.keybinds.volume_down.matches(key) {
            self.adjust_volume(-5);
            return;
        }
        if self.keybinds.volume_up.matches(key) {
            self.adjust_volume(5);
            return;
        }

        if self.interaction_mode == InteractionMode::Queue {
            self.on_queue_mode_key(key);
            return;
        }

        if matches!(self.input_mode, InputMode::Search { .. }) {
            self.on_search_key(key);
            return;
        }

        if self.keybinds.search_backspace.matches(key) && self.backspace_active_filter() {
            return;
        }

        if self.keybinds.search.matches(key) {
            self.begin_search();
            return;
        }
        if self.keybinds.tab_artists.matches(key) {
            self.switch_to_tab(Tab::Artists);
            return;
        }
        if self.keybinds.tab_albums.matches(key) {
            self.switch_to_tab(Tab::Albums);
            return;
        }
        if self.keybinds.tab_songs.matches(key) {
            self.switch_to_tab(Tab::Songs);
            return;
        }
        if self.keybinds.tab_cycle.matches(key) {
            self.cycle_tab();
            return;
        }
        if self.keybinds.nav_up.matches(key) {
            self.handle_nav(Action::Up);
            return;
        }
        if self.keybinds.nav_down.matches(key) {
            self.handle_nav(Action::Down);
            return;
        }
        if self.keybinds.activate.matches(key) {
            self.handle_nav(Action::RightOrEnter);
            return;
        }
        if self.keybinds.nav_left.matches(key) {
            self.handle_nav(Action::Left);
            return;
        }
        if self.keybinds.enqueue.matches(key) {
            self.enqueue_selected_item();
            return;
        }
        if self.keybinds.play_next.matches(key) {
            self.play_next_from_browser_selection();
            return;
        }
        if self.keybinds.play_pause.matches(key) {
            self.toggle_pause();
            return;
        }
        if self.keybinds.clear_queue.matches(key) {
            self.clear_queue();
            return;
        }
        if self.keybinds.seek_back.matches(key) {
            self.seek_relative_seconds(-10.0);
            return;
        }
        if self.keybinds.seek_forward.matches(key) {
            self.seek_relative_seconds(10.0);
            return;
        }
        if self.keybinds.hard_refresh.matches(key) {
            self.hard_refresh_library();
            return;
        }
        if self.keybinds.shuffle.matches(key) {
            if let Err(err) = self.shuffle_current_context() {
                self.status = format!("Shuffle failed: {err}");
            }
            return;
        }
        if self.keybinds.queue_back.matches(key) {
            self.queue_back();
            return;
        }
        if self.keybinds.queue_forward.matches(key) {
            self.queue_forward();
            return;
        }
    }

    fn repeat_allowed_for(&self, key: KeyEvent) -> bool {
        self.keybinds.nav_up.matches(key)
            || self.keybinds.nav_down.matches(key)
            || self.keybinds.nav_left.matches(key)
            || (self.keybinds.activate.matches(key) && key.code != KeyCode::Enter)
            || self.keybinds.volume_down.matches(key)
            || self.keybinds.volume_up.matches(key)
            || self.keybinds.seek_back.matches(key)
            || self.keybinds.seek_forward.matches(key)
    }

    fn on_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {
                if self.set_volume_from_mouse(mouse.column, mouse.row) {
                    return;
                }
                self.set_timeline_from_mouse(mouse.column, mouse.row);
            }
            MouseEventKind::ScrollUp => {
                self.scroll_queue_at(mouse.column, mouse.row, -1);
                self.scroll_main_at(mouse.column, mouse.row, Action::Up);
            }
            MouseEventKind::ScrollDown => {
                self.scroll_queue_at(mouse.column, mouse.row, 1);
                self.scroll_main_at(mouse.column, mouse.row, Action::Down);
            }
            _ => {}
        }
    }

    fn scroll_queue_at(&mut self, column: u16, row: u16, delta: i16) {
        if !self.point_in_queue(column, row) {
            return;
        }
        self.scroll_queue(delta);
    }

    fn scroll_main_at(&mut self, column: u16, row: u16, action: Action) {
        if self.interaction_mode != InteractionMode::Browser {
            return;
        }
        if !self.point_in_main(column, row) {
            return;
        }
        self.handle_nav(action);
    }

    fn point_in_main(&self, column: u16, row: u16) -> bool {
        let Some(area) = self.main_hitbox else {
            return false;
        };
        let max_x = area.x.saturating_add(area.width);
        let max_y = area.y.saturating_add(area.height);
        column >= area.x && column < max_x && row >= area.y && row < max_y
    }

    fn point_in_queue(&self, column: u16, row: u16) -> bool {
        let Some(area) = self.queue_hitbox else {
            return false;
        };
        let max_x = area.x.saturating_add(area.width);
        let max_y = area.y.saturating_add(area.height);
        column >= area.x && column < max_x && row >= area.y && row < max_y
    }

    fn scroll_queue(&mut self, delta: i16) {
        self.queue_follow_index = None;
        let max_offset = self.max_queue_scroll_offset();
        if max_offset == 0 {
            self.queue_scroll_offset = 0;
            return;
        }

        let current = self.queue_scroll_offset as i32;
        let next = (current + i32::from(delta)).clamp(0, max_offset as i32) as usize;
        self.queue_scroll_offset = next;
    }

    fn max_queue_scroll_offset(&self) -> usize {
        if self.queue_visible_rows == 0 {
            return 0;
        }
        self.queue.len().saturating_sub(self.queue_visible_rows)
    }

    fn on_queue_mode_key(&mut self, key: KeyEvent) {
        if self.keybinds.nav_up.matches(key) {
            if self.queue_reorder_index.is_some() {
                self.queue_reorder_move(-1);
            } else {
                self.queue_move_cursor(-1);
            }
            return;
        }
        if self.keybinds.nav_down.matches(key) {
            if self.queue_reorder_index.is_some() {
                self.queue_reorder_move(1);
            } else {
                self.queue_move_cursor(1);
            }
            return;
        }
        if self.keybinds.activate.matches(key) {
            if self.queue_reorder_index.is_none() {
                self.queue_play_cursor();
            }
            return;
        }
        if self.keybinds.queue_reorder_toggle.matches(key) {
            self.toggle_queue_reorder_mode();
            return;
        }
        if self.keybinds.queue_back.matches(key) {
            self.queue_back();
            return;
        }
        if self.keybinds.queue_forward.matches(key) {
            self.queue_forward();
            return;
        }
        if self.keybinds.queue_remove.matches(key) {
            self.remove_selected_queue_item();
            return;
        }
        if self.keybinds.play_next.matches(key) {
            if self.queue_reorder_index.is_some() {
                self.status = "Drop/cancel queue move before play-next".to_string();
            } else {
                self.play_next_from_queue_selection();
            }
            return;
        }
        if self.keybinds.seek_back.matches(key) {
            self.seek_relative_seconds(-10.0);
            return;
        }
        if self.keybinds.seek_forward.matches(key) {
            self.seek_relative_seconds(10.0);
            return;
        }
        if self.keybinds.clear_queue.matches(key) {
            self.clear_queue();
        }
    }

    fn toggle_interaction_mode(&mut self) {
        if matches!(self.input_mode, InputMode::Search { .. }) {
            self.input_mode = InputMode::Normal;
        }

        self.interaction_mode = match self.interaction_mode {
            InteractionMode::Browser => InteractionMode::Queue,
            InteractionMode::Queue => InteractionMode::Browser,
        };

        if self.interaction_mode == InteractionMode::Queue {
            if self.queue.is_empty() {
                self.queue_nav_index = None;
                self.queue_reorder_index = None;
                self.queue_reorder_snapshot = None;
            } else {
                let nav = self
                    .queue_index
                    .unwrap_or(0)
                    .min(self.queue.len().saturating_sub(1));
                self.queue_nav_index = Some(nav);
                self.queue_follow_index = Some(nav);
                self.queue_reorder_index = None;
                self.queue_reorder_snapshot = None;
            }
        } else {
            self.queue_reorder_index = None;
            self.queue_reorder_snapshot = None;
        }
    }

    fn queue_move_cursor(&mut self, delta: i8) {
        if self.queue.is_empty() {
            self.queue_nav_index = None;
            self.status = "Queue is empty".to_string();
            return;
        }

        let len = self.queue.len();
        let current = self
            .queue_nav_index
            .or(self.queue_index)
            .unwrap_or(if delta >= 0 { 0 } else { len - 1 });
        let next = if delta < 0 {
            if current == 0 { len - 1 } else { current - 1 }
        } else if current + 1 >= len {
            0
        } else {
            current + 1
        };

        self.queue_nav_index = Some(next);
        self.queue_follow_index = Some(next);
    }

    fn queue_play_cursor(&mut self) {
        let Some(selected_index) = self.queue_nav_index.or(self.queue_index) else {
            self.status = "Queue is empty".to_string();
            return;
        };
        if selected_index >= self.queue.len() {
            self.status = "Queue selection out of range".to_string();
            return;
        }

        self.set_queue_index(Some(selected_index));
        if let Err(err) = self.play_current_queue_song() {
            self.status = format!("Playback failed: {err}");
        }
    }

    fn remove_selected_queue_item(&mut self) {
        let Some(selected_index) = self
            .queue_reorder_index
            .or(self.queue_nav_index)
            .or(self.queue_index)
        else {
            self.status = "Queue is empty".to_string();
            return;
        };
        if selected_index >= self.queue.len() {
            self.status = "Queue selection out of range".to_string();
            return;
        }

        let removed = self.queue.remove(selected_index);

        // Deleting during active reorder finalizes that mode.
        self.queue_reorder_index = None;
        self.queue_reorder_snapshot = None;

        if let Some(playing_index) = self.queue_index {
            if playing_index == selected_index {
                if self.queue.is_empty() {
                    self.set_queue_index(None);
                    self.failed_retry_song_id = None;
                    match self.player.stop() {
                        Ok(()) => {
                            self.status = format!(
                                "Removed from queue: {} - {} (queue empty)",
                                removed.artist_name, removed.title
                            );
                        }
                        Err(err) => {
                            self.status = format!(
                                "Removed current track, but failed to stop playback: {err}"
                            );
                        }
                    }
                    return;
                }

                let next_index = selected_index.min(self.queue.len().saturating_sub(1));
                self.set_queue_index(Some(next_index));
                self.failed_retry_song_id = None;
                match self.play_current_queue_song() {
                    Ok(()) => {
                        self.status = format!(
                            "Removed current track: {} - {}",
                            removed.artist_name, removed.title
                        );
                    }
                    Err(err) => {
                        self.status = format!("Removed current track, but playback failed: {err}");
                    }
                }
                return;
            }

            if playing_index > selected_index {
                self.queue_index = Some(playing_index - 1);
            }
        }

        if self.queue.is_empty() {
            self.queue_nav_index = None;
            self.queue_follow_index = None;
            self.queue_scroll_offset = 0;
            self.status = format!(
                "Removed from queue: {} - {} (queue empty)",
                removed.artist_name, removed.title
            );
            return;
        }

        let next_nav = selected_index.min(self.queue.len().saturating_sub(1));
        self.queue_nav_index = Some(next_nav);
        self.queue_follow_index = Some(next_nav);
        let max_offset = self.max_queue_scroll_offset();
        if self.queue_scroll_offset > max_offset {
            self.queue_scroll_offset = max_offset;
        }

        self.status = format!(
            "Removed from queue: {} - {}",
            removed.artist_name, removed.title
        );
    }

    fn toggle_queue_reorder_mode(&mut self) {
        if let Some(index) = self.queue_reorder_index.take() {
            self.queue_reorder_snapshot = None;
            self.queue_nav_index = Some(index);
            self.queue_follow_index = Some(index);
            self.status = "Queue item placed".to_string();
            return;
        }

        if self.queue.is_empty() {
            self.status = "Queue is empty".to_string();
            return;
        }

        let index = self
            .queue_nav_index
            .or(self.queue_index)
            .unwrap_or(0)
            .min(self.queue.len().saturating_sub(1));
        self.queue_reorder_snapshot = Some(QueueReorderSnapshot {
            queue: self.queue.clone(),
            original_index: index,
        });
        self.queue_reorder_index = Some(index);
        self.queue_nav_index = Some(index);
        self.queue_follow_index = Some(index);
        self.status = "Queue reorder active (Up/Down move, Space drop)".to_string();
    }

    fn cancel_queue_reorder_mode(&mut self) {
        let Some(snapshot) = self.queue_reorder_snapshot.take() else {
            self.queue_reorder_index = None;
            self.status = "Queue reorder canceled".to_string();
            return;
        };

        let current_song_id = self
            .queue_index
            .and_then(|index| self.queue.get(index))
            .map(|song| song.id.clone());

        self.queue = snapshot.queue;
        self.queue_reorder_index = None;
        self.queue_index = current_song_id
            .as_ref()
            .and_then(|song_id| self.queue.iter().position(|song| &song.id == song_id));

        if self.queue.is_empty() {
            self.queue_nav_index = None;
            self.queue_follow_index = None;
            self.queue_scroll_offset = 0;
        } else {
            let nav = snapshot
                .original_index
                .min(self.queue.len().saturating_sub(1));
            self.queue_nav_index = Some(nav);
            self.queue_follow_index = Some(nav);
            let max_offset = self.max_queue_scroll_offset();
            if self.queue_scroll_offset > max_offset {
                self.queue_scroll_offset = max_offset;
            }
        }

        self.status = "Queue move canceled".to_string();
    }

    fn queue_reorder_move(&mut self, delta: i8) {
        let Some(current_index) = self.queue_reorder_index else {
            return;
        };
        if self.queue.len() < 2 {
            return;
        }

        let len = self.queue.len();
        let new_index = if delta < 0 {
            if current_index == 0 {
                len - 1
            } else {
                current_index - 1
            }
        } else if current_index + 1 >= len {
            0
        } else {
            current_index + 1
        };

        if new_index == current_index {
            return;
        }

        let moved = self.queue.remove(current_index);
        self.queue.insert(new_index, moved);
        if let Some(playing) = self.queue_index {
            if playing == current_index {
                self.queue_index = Some(new_index);
            } else if current_index < new_index {
                // Moving an item downward (including top->bottom wrap) shifts intervening
                // indices left by one.
                if playing > current_index && playing <= new_index {
                    self.queue_index = Some(playing - 1);
                }
            } else {
                // Moving an item upward (including bottom->top wrap) shifts intervening
                // indices right by one.
                if playing >= new_index && playing < current_index {
                    self.queue_index = Some(playing + 1);
                }
            }
        }

        self.queue_reorder_index = Some(new_index);
        self.queue_nav_index = Some(new_index);
        self.queue_follow_index = Some(new_index);
    }

    fn set_queue_index(&mut self, new_index: Option<usize>) {
        self.queue_index = new_index;
        if let Some(index) = self.queue_index {
            self.queue_nav_index = Some(index);
            self.queue_follow_index = Some(index);
        } else {
            self.queue_nav_index = None;
            self.queue_reorder_index = None;
            self.queue_reorder_snapshot = None;
            self.queue_scroll_offset = 0;
            self.queue_follow_index = None;
            self.stop_playback_clock();
        }
    }

    fn ensure_queue_row_visible(&mut self, index: usize) {
        if self.queue_visible_rows == 0 {
            return;
        }

        if index < self.queue_scroll_offset {
            self.queue_scroll_offset = index;
        } else {
            let visible_end = self
                .queue_scroll_offset
                .saturating_add(self.queue_visible_rows);
            if index >= visible_end {
                self.queue_scroll_offset = index
                    .saturating_add(1)
                    .saturating_sub(self.queue_visible_rows);
            }
        }

        let max_offset = self.max_queue_scroll_offset();
        if self.queue_scroll_offset > max_offset {
            self.queue_scroll_offset = max_offset;
        }
    }

    fn begin_search(&mut self) {
        let existing = self.browser.active_filter().to_string();
        self.input_mode = InputMode::Search {
            buffer: existing.clone(),
        };
        self.status = format!("Search /{existing} (live, Esc clear/back, Ctrl+Esc reset)");
    }

    fn on_search_key(&mut self, key: KeyEvent) {
        enum SearchOp {
            None,
            ApplyLive(String),
            ExitSearch,
            ExitSearchAndApply(String),
            SelectCurrent,
            Navigate(Action),
            CancelAndTab,
        }

        let op = if let InputMode::Search { buffer } = &mut self.input_mode {
            if self.keybinds.tab_cycle.matches(key) {
                SearchOp::CancelAndTab
            } else if self.keybinds.nav_up.matches(key) {
                SearchOp::Navigate(Action::Up)
            } else if self.keybinds.nav_down.matches(key) {
                SearchOp::Navigate(Action::Down)
            } else if self.keybinds.nav_left.matches(key) {
                SearchOp::Navigate(Action::Left)
            } else if self.keybinds.activate.matches(key) {
                SearchOp::SelectCurrent
            } else if self.keybinds.search_backspace.matches(key) {
                if buffer.is_empty() {
                    SearchOp::ExitSearch
                } else {
                    buffer.pop();
                    if buffer.is_empty() {
                        SearchOp::ExitSearchAndApply(String::new())
                    } else {
                        SearchOp::ApplyLive(buffer.clone())
                    }
                }
            } else if let KeyCode::Char(c) = key.code {
                buffer.push(c);
                SearchOp::ApplyLive(buffer.clone())
            } else {
                SearchOp::None
            }
        } else {
            SearchOp::None
        };

        match op {
            SearchOp::None => {}
            SearchOp::ApplyLive(query) => {
                self.apply_live_search(query);
            }
            SearchOp::ExitSearch => {
                self.input_mode = InputMode::Normal;
                self.status = "Search exited".to_string();
            }
            SearchOp::ExitSearchAndApply(query) => {
                self.input_mode = InputMode::Normal;
                self.apply_filter_query(query, false);
            }
            SearchOp::SelectCurrent => {
                self.input_mode = InputMode::Normal;
                self.handle_nav(Action::RightOrEnter);
            }
            SearchOp::Navigate(action) => {
                self.handle_nav(action);
            }
            SearchOp::CancelAndTab => {
                self.input_mode = InputMode::Normal;
                self.cycle_tab();
            }
        }
    }

    fn cycle_tab(&mut self) {
        let target = self.browser.active_tab().next();
        self.switch_to_tab(target);
    }

    fn switch_to_tab(&mut self, target: Tab) {
        if self.should_use_cached_tab_switch(target) {
            self.browser.go_to_tab_loaded(target, &self.cache);
            let label = match target {
                Tab::Artists => "artists",
                Tab::Albums => "albums",
                Tab::Songs => "songs",
            };
            self.status = format!(
                "Library warmup in progress; showing cached {label} ({})",
                self.browser.active_len()
            );
            return;
        }

        match self
            .browser
            .go_to_tab(target, &mut self.cache, &self.client)
        {
            Ok(()) => {}
            Err(err) => self.status = format!("Navigation failed: {err}"),
        }
    }

    fn should_use_cached_tab_switch(&self, target: Tab) -> bool {
        if self.library_warmup.is_none() {
            return false;
        }
        match target {
            Tab::Artists => false,
            Tab::Albums => !self.cache.has_all_albums_loaded(),
            Tab::Songs => !self.cache.has_all_songs_loaded(),
        }
    }

    fn reset_to_artists_home(&mut self) {
        self.input_mode = InputMode::Normal;
        self.interaction_mode = InteractionMode::Browser;
        self.queue_reorder_index = None;
        self.queue_reorder_snapshot = None;
        match self
            .browser
            .go_to_tab(Tab::Artists, &mut self.cache, &self.client)
        {
            Ok(()) => self.status = "Reset to Artists".to_string(),
            Err(err) => self.status = format!("Reset failed: {err}"),
        }
    }

    fn handle_escape(&mut self) {
        match &mut self.input_mode {
            InputMode::Search { buffer } => {
                if !buffer.is_empty() || !self.browser.active_filter().is_empty() {
                    buffer.clear();
                    self.apply_filter_query(String::new(), true);
                    self.status = "Search cleared (Esc again to go back)".to_string();
                } else {
                    self.input_mode = InputMode::Normal;
                    self.handle_nav(Action::Left);
                }
            }
            InputMode::Normal => {
                if !self.browser.active_filter().is_empty() {
                    self.apply_filter_query(String::new(), false);
                    self.status = "Search cleared (Esc again to go back)".to_string();
                } else {
                    self.handle_nav(Action::Left);
                }
            }
        }
    }

    fn backspace_active_filter(&mut self) -> bool {
        let mut buffer = self.browser.active_filter().to_string();
        if buffer.is_empty() {
            return false;
        }

        buffer.pop();
        if buffer.is_empty() {
            self.input_mode = InputMode::Normal;
            self.apply_filter_query(String::new(), false);
        } else {
            self.input_mode = InputMode::Search {
                buffer: buffer.clone(),
            };
            self.apply_live_search(buffer);
        }
        true
    }

    fn apply_live_search(&mut self, query: String) {
        self.apply_filter_query(query, true);
        if self.expand_on_search_collapse && self.browser.active_len() == 1 {
            match self.browser.active_tab() {
                Tab::Artists | Tab::Albums => {
                    self.handle_nav(Action::RightOrEnter);
                    self.input_mode = InputMode::Normal;
                    self.apply_filter_query(String::new(), false);
                }
                Tab::Songs => {}
            }
        }
    }

    fn adjust_volume(&mut self, delta_percent: i16) {
        let old = self.volume_percent;
        let next = (old as i16 + delta_percent).clamp(0, 100) as u8;
        self.set_volume_percent(next);
    }

    fn set_volume_from_mouse(&mut self, column: u16, row: u16) -> bool {
        let Some(hitbox) = self.volume_slider_hitbox else {
            return false;
        };
        if row != hitbox.y || column < hitbox.x || column >= hitbox.x + hitbox.width {
            return false;
        }

        let bar_index = column - hitbox.x;
        let raw_volume = if hitbox.width <= 1 {
            100
        } else {
            ((u32::from(bar_index) * 100) / u32::from(hitbox.width - 1)) as u8
        };
        let new_volume = ((raw_volume as u16 + 2) / 5 * 5).min(100) as u8;
        self.set_volume_percent(new_volume);
        true
    }

    fn set_timeline_from_mouse(&mut self, column: u16, row: u16) -> bool {
        let Some(hitbox) = self.timeline_slider_hitbox else {
            return false;
        };
        if row != hitbox.y || column < hitbox.x || column >= hitbox.x + hitbox.width {
            return false;
        }

        let Some(song) = self.current_song() else {
            return false;
        };
        let Some(total_seconds) = song.duration_seconds else {
            return false;
        };
        if total_seconds == 0 {
            return false;
        }

        let bar_index = column - hitbox.x;
        let ratio = if hitbox.width <= 1 {
            1.0
        } else {
            f64::from(bar_index) / f64::from(hitbox.width - 1)
        };
        let target_seconds = ratio * total_seconds as f64;
        self.seek_current_song_to(target_seconds);
        true
    }

    fn set_volume_percent(&mut self, new_volume: u8) {
        let new_volume = new_volume.clamp(0, 100);
        if new_volume == self.volume_percent {
            return;
        }

        if self.current_song().is_none() {
            self.volume_percent = new_volume;
            return;
        }
        if !self.player.has_active_playback() {
            self.volume_percent = new_volume;
            return;
        }

        match self.player.set_live_volume(new_volume) {
            Ok(true) => {
                self.volume_percent = new_volume;
            }
            Ok(false) => {
                self.status = "Volume update pending until audio backend is ready".to_string();
            }
            Err(err) => {
                self.status = format!("Volume apply failed: {err}");
            }
        }
    }

    fn playback_elapsed_seconds(&self) -> f64 {
        let mut elapsed = self.playback_position_seconds;
        if let Some(anchor) = self.playback_anchor_instant {
            elapsed += anchor.elapsed().as_secs_f64();
        }
        elapsed.max(0.0)
    }

    fn reset_playback_clock(&mut self, seek_seconds: f64) {
        self.playback_position_seconds = seek_seconds.max(0.0);
        self.playback_anchor_instant = Some(Instant::now());
    }

    fn pause_playback_clock(&mut self) {
        if let Some(anchor) = self.playback_anchor_instant.take() {
            self.playback_position_seconds += anchor.elapsed().as_secs_f64();
        }
    }

    fn resume_playback_clock(&mut self) {
        if self.playback_anchor_instant.is_none() {
            self.playback_anchor_instant = Some(Instant::now());
        }
    }

    fn stop_playback_clock(&mut self) {
        self.playback_position_seconds = 0.0;
        self.playback_anchor_instant = None;
    }

    fn seek_relative_seconds(&mut self, delta_seconds: f64) {
        let current_pos = self.playback_elapsed_seconds();
        self.seek_current_song_to(current_pos + delta_seconds);
    }

    fn seek_current_song_to(&mut self, target_seconds: f64) {
        let Some(song) = self.current_song().cloned() else {
            self.status = "Nothing to seek".to_string();
            return;
        };
        if !self.player.has_active_playback() {
            self.status = "Nothing to seek".to_string();
            return;
        }

        let was_paused = self.player.paused();
        let max_seek = song
            .duration_seconds
            .map(|seconds| (seconds as f64 - 0.05).max(0.0));
        let mut target_pos = target_seconds.max(0.0);
        if let Some(max_seek) = max_seek {
            target_pos = target_pos.min(max_seek);
        }

        match self.play_song_direct_seek(&song, target_pos) {
            Ok(()) => {
                if was_paused {
                    match self.player.toggle_pause() {
                        Ok(true) => self.pause_playback_clock(),
                        Ok(false) => self.resume_playback_clock(),
                        Err(err) => {
                            self.status = format!("Seek applied, but pause restore failed: {err}");
                            return;
                        }
                    }
                }
                let elapsed_label = format_timestamp(target_pos);
                if let Some(total) = song.duration_seconds {
                    self.status = format!(
                        "Seeked to {elapsed_label} / {}",
                        format_timestamp(total as f64)
                    );
                } else {
                    self.status = format!("Seeked to {elapsed_label}");
                }
            }
            Err(err) => {
                self.status = format!("Seek failed: {err}");
            }
        }
    }

    fn apply_filter_query(&mut self, query: String, live: bool) {
        if self.should_use_cached_filter_path() {
            self.browser
                .set_filter_for_active_tab_loaded(query.clone(), &self.cache);
            let total = self.browser.active_len();
            if let InputMode::Search { .. } = &self.input_mode {
                if total == 1 {
                    self.status = format!("Search /{query} -> 1 match (auto-selected)");
                } else {
                    self.status = format!("Search /{query} -> {total} cached match(es)");
                }
            } else if self.browser.active_filter().is_empty() {
                self.status = "Filter cleared".to_string();
            } else if total == 1 {
                self.status = "1 match (auto-selected)".to_string();
            } else if live {
                self.status = format!("Search /{query} -> {total} cached match(es)");
            } else {
                self.status = format!("Cached filter matches: {total}");
            }
            return;
        }

        match self
            .browser
            .set_filter_for_active_tab(query.clone(), &mut self.cache, &self.client)
        {
            Ok(()) => {
                let total = self.browser.active_len();
                if let InputMode::Search { .. } = &self.input_mode {
                    if total == 1 {
                        self.status = format!("Search /{query} -> 1 match (auto-selected)");
                    } else {
                        self.status = format!("Search /{query} -> {total} match(es)");
                    }
                } else if self.browser.active_filter().is_empty() {
                    self.status = "Filter cleared".to_string();
                } else if total == 1 {
                    self.status = "1 match (auto-selected)".to_string();
                } else if live {
                    self.status = format!("Search /{query} -> {total} match(es)");
                } else {
                    self.status = format!("Filter matches: {total}");
                }
            }
            Err(err) => {
                self.status = format!("Filter failed: {err}");
            }
        }
    }

    fn should_use_cached_filter_path(&self) -> bool {
        if self.library_warmup.is_none() {
            return false;
        }
        match self.browser.active_tab() {
            Tab::Artists => false,
            Tab::Albums => self.browser.is_album_scope_all() && !self.cache.has_all_albums_loaded(),
            Tab::Songs => self.browser.is_song_scope_all() && !self.cache.has_all_songs_loaded(),
        }
    }

    fn handle_nav(&mut self, action: Action) {
        match self
            .browser
            .handle_action(action, &mut self.cache, &self.client)
        {
            Ok(Outcome::None) => {}
            Ok(Outcome::Play(song)) => self.play_song(song),
            Err(err) => self.status = format!("Navigation failed: {err}"),
        }
    }

    fn play_song(&mut self, selected: Song) {
        if let Err(err) = self.play_song_direct(&selected) {
            self.status = format!("Playback failed: {err}");
            return;
        }

        let source = self.browser.songs().to_vec();
        if source.is_empty() {
            self.queue = vec![selected];
            self.set_queue_index(Some(0));
        } else {
            self.queue = source;
            let next = self
                .queue
                .iter()
                .position(|song| song.id == selected.id)
                .or(Some(0));
            self.set_queue_index(next);
        }
    }

    fn queue_back(&mut self) {
        match self.queue_index {
            Some(index) if index > 0 => {
                self.set_queue_index(Some(index - 1));
                if let Err(err) = self.play_current_queue_song() {
                    self.status = format!("Playback failed: {err}");
                }
            }
            _ => {
                self.status = "Already at start of queue".to_string();
            }
        }
    }

    fn queue_forward(&mut self) {
        match self.queue_index {
            Some(index) if index + 1 < self.queue.len() => {
                self.set_queue_index(Some(index + 1));
                if let Err(err) = self.play_current_queue_song() {
                    self.status = format!("Playback failed: {err}");
                }
            }
            _ => {
                self.status = "Already at end of queue".to_string();
            }
        }
    }

    fn clear_queue(&mut self) {
        self.queue.clear();
        self.set_queue_index(None);
        self.stop_playback_clock();
        if let Err(err) = self.player.stop() {
            self.status = format!("Failed to stop playback: {err}");
        } else {
            self.status = "Queue cleared".to_string();
        }
    }

    fn hard_refresh_library(&mut self) {
        let clear_library_result =
            disk_cache::clear_library_snapshot(self.client.server_url(), self.client.username())
                .err();
        let clear_dns_result = disk_cache::clear_dns_cache().err();

        let fresh_cache = match LibraryCache::load(&self.client) {
            Ok(cache) => cache,
            Err(err) => {
                if let Some(clear_err) = clear_library_result.as_ref() {
                    self.status = format!(
                        "Hard refresh failed after cache clear warning: {clear_err}; fetch error: {err}"
                    );
                } else if let Some(clear_dns_err) = clear_dns_result.as_ref() {
                    self.status = format!(
                        "Hard refresh failed after DNS cache clear warning: {clear_dns_err}; fetch error: {err}"
                    );
                } else {
                    self.status = format!("Hard refresh failed: {err}");
                }
                return;
            }
        };

        let stop_error = self.player.stop().err();

        self.queue.clear();
        self.set_queue_index(None);
        self.failed_retry_song_id = None;
        self.stop_playback_clock();
        self.cache = fresh_cache;
        self.browser = BrowserState::new(self.cache.artists().to_vec());

        let artists = self.cache.artists().len();
        match disk_cache::save_library_snapshot(
            self.client.server_url(),
            self.client.username(),
            &self.cache.snapshot(),
        ) {
            Ok(()) => {
                if let Some(err) = stop_error {
                    self.status =
                        format!("Hard refresh complete ({artists} artists); stop warning: {err}");
                } else if let Some(clear_err) = clear_library_result.as_ref() {
                    self.status = format!(
                        "Hard refresh complete ({artists} artists); cache clear warning: {clear_err}"
                    );
                } else if let Some(clear_dns_err) = clear_dns_result.as_ref() {
                    self.status = format!(
                        "Hard refresh complete ({artists} artists); DNS cache clear warning: {clear_dns_err}"
                    );
                } else {
                    self.status = format!("Hard refresh complete ({artists} artists)");
                }
            }
            Err(err) => {
                if let Some(stop_err) = stop_error {
                    self.status = format!(
                        "Hard refresh complete ({artists} artists); snapshot save warning: {err}; stop warning: {stop_err}"
                    );
                } else if let Some(clear_err) = clear_library_result.as_ref() {
                    self.status = format!(
                        "Hard refresh complete ({artists} artists); snapshot save warning: {err}; cache clear warning: {clear_err}"
                    );
                } else if let Some(clear_dns_err) = clear_dns_result.as_ref() {
                    self.status = format!(
                        "Hard refresh complete ({artists} artists); snapshot save warning: {err}; DNS cache clear warning: {clear_dns_err}"
                    );
                } else {
                    self.status = format!(
                        "Hard refresh complete ({artists} artists); snapshot save warning: {err}"
                    );
                }
            }
        }

        self.start_background_library_warmup("refresh");
    }

    fn enqueue_selected_item(&mut self) {
        match self.browser.active_tab() {
            Tab::Artists => self.enqueue_selected_artist(),
            Tab::Songs => self.enqueue_selected_song(),
            Tab::Albums => self.enqueue_selected_album(),
        }
    }

    fn play_next_from_browser_selection(&mut self) {
        match self.browser.active_tab() {
            Tab::Artists => self.play_next_selected_artist(),
            Tab::Albums => self.play_next_selected_album(),
            Tab::Songs => self.play_next_selected_song(),
        }
    }

    fn play_next_from_queue_selection(&mut self) {
        let Some(selected_index) = self.queue_nav_index.or(self.queue_index) else {
            self.status = "Queue is empty".to_string();
            return;
        };

        if selected_index >= self.queue.len() {
            self.status = "Queue selection out of range".to_string();
            return;
        }

        if let Some(current_index) = self.queue_index {
            if selected_index == current_index {
                self.status = "Selected track is already playing".to_string();
                return;
            }

            if selected_index == current_index + 1 {
                self.queue_nav_index = Some(selected_index);
                self.queue_follow_index = Some(selected_index);
                self.status = "Selected track is already next".to_string();
                return;
            }
        }

        let moved_song = self.queue.remove(selected_index);

        if let Some(current_index) = self.queue_index
            && current_index > selected_index
        {
            self.queue_index = Some(current_index - 1);
        }

        let insert_at = self.play_next_insert_index();
        self.queue.insert(insert_at, moved_song.clone());
        self.queue_nav_index = Some(insert_at);
        self.queue_follow_index = Some(insert_at);

        if self.queue_index.is_none() {
            self.set_queue_index(Some(insert_at));
            match self.play_current_queue_song() {
                Ok(()) => {
                    self.status = format!(
                        "Play-next started: {} - {}",
                        moved_song.artist_name, moved_song.title
                    );
                }
                Err(err) => {
                    self.status = format!(
                        "Play-next queued: {} - {}, but playback failed: {err}",
                        moved_song.artist_name, moved_song.title
                    );
                }
            }
            return;
        }

        self.status = format!(
            "Will play next: {} - {}",
            moved_song.artist_name, moved_song.title
        );
    }

    fn play_next_insert_index(&self) -> usize {
        match self.queue_index {
            Some(index) => (index + 1).min(self.queue.len()),
            None => 0,
        }
    }

    fn queue_songs_play_next(&mut self, songs: Vec<Song>) -> Result<bool> {
        if songs.is_empty() {
            return Ok(false);
        }

        let was_idle = self.queue_index.is_none();
        let insert_at = self.play_next_insert_index();
        let added = songs.len();
        self.queue.splice(insert_at..insert_at, songs);
        self.queue_follow_index = Some(insert_at);
        if self.interaction_mode == InteractionMode::Queue {
            self.queue_nav_index = Some(insert_at);
        }

        if let Some(current_index) = self.queue_index
            && current_index >= insert_at
        {
            self.queue_index = Some(current_index + added);
        }

        if was_idle {
            self.set_queue_index(Some(insert_at));
            self.play_current_queue_song()?;
            return Ok(true);
        }

        Ok(false)
    }

    fn play_next_selected_artist(&mut self) {
        let Some(artist) = self.browser.selected_artist().cloned() else {
            self.status = "Select an artist for play-next (Artists tab)".to_string();
            return;
        };

        match self.collect_artist_songs(&artist.id) {
            Ok(songs) if songs.is_empty() => {
                self.status = format!("No songs found for artist: {}", artist.name);
            }
            Ok(songs) => {
                let added = songs.len();
                match self.queue_songs_play_next(songs) {
                    Ok(true) => {
                        self.status = format!(
                            "Play-next artist and started playback: {} ({added} songs)",
                            artist.name
                        );
                    }
                    Ok(false) => {
                        self.status = format!("Play-next artist: {} ({added} songs)", artist.name);
                    }
                    Err(err) => {
                        self.status = format!(
                            "Play-next artist: {} ({added} songs), but playback failed: {err}",
                            artist.name
                        );
                    }
                }
            }
            Err(err) => {
                self.status = format!("Failed to queue play-next artist: {err}");
            }
        }
    }

    fn play_next_selected_song(&mut self) {
        let Some(song) = self.browser.selected_song().cloned() else {
            self.status = "Select a song for play-next (Songs tab)".to_string();
            return;
        };

        match self.queue_songs_play_next(vec![song.clone()]) {
            Ok(true) => {
                self.status = format!(
                    "Play-next song and started playback: {} - {}",
                    song.artist_name, song.title
                );
            }
            Ok(false) => {
                self.status = format!("Play-next song: {} - {}", song.artist_name, song.title);
            }
            Err(err) => {
                self.status = format!(
                    "Play-next song: {} - {}, but playback failed: {err}",
                    song.artist_name, song.title
                );
            }
        }
    }

    fn play_next_selected_album(&mut self) {
        let Some(album) = self.browser.selected_album().cloned() else {
            self.status = "Select an album for play-next (Albums tab)".to_string();
            return;
        };

        match self.cache.songs_for_album(&self.client, &album.id) {
            Ok(songs) if songs.is_empty() => {
                self.status = format!("No songs found for album: {}", album.title);
            }
            Ok(songs) => {
                let songs = songs.to_vec();
                let added = songs.len();
                match self.queue_songs_play_next(songs) {
                    Ok(true) => {
                        self.status = format!(
                            "Play-next album and started playback: {} ({added} songs)",
                            album.title
                        );
                    }
                    Ok(false) => {
                        self.status = format!("Play-next album: {} ({added} songs)", album.title);
                    }
                    Err(err) => {
                        self.status = format!(
                            "Play-next album: {} ({added} songs), but playback failed: {err}",
                            album.title
                        );
                    }
                }
            }
            Err(err) => {
                self.status = format!("Failed to queue play-next album: {err}");
            }
        }
    }

    fn enqueue_selected_artist(&mut self) {
        let Some(artist) = self.browser.selected_artist().cloned() else {
            self.status = "Select an artist to queue (Artists tab)".to_string();
            return;
        };

        match self.collect_artist_songs(&artist.id) {
            Ok(songs) if songs.is_empty() => {
                self.status = format!("No songs found for artist: {}", artist.name);
            }
            Ok(songs) => {
                let added = songs.len();
                let start_index = self.queue.len();
                self.queue.extend(songs.into_iter());
                if self.queue_index.is_none() {
                    self.set_queue_index(Some(start_index));
                    match self.play_current_queue_song() {
                        Ok(()) => {
                            self.status = format!(
                                "Queued artist and started playback: {} ({added} songs)",
                                artist.name
                            );
                        }
                        Err(err) => {
                            self.status = format!(
                                "Queued artist: {} ({added} songs), but playback failed: {err}",
                                artist.name
                            );
                        }
                    }
                } else {
                    self.status = format!("Queued artist: {} ({added} songs)", artist.name);
                }
            }
            Err(err) => {
                self.status = format!("Failed to queue artist: {err}");
            }
        }
    }

    fn enqueue_selected_song(&mut self) {
        let Some(song) = self.browser.selected_song().cloned() else {
            self.status = "Select a song to queue (Songs tab)".to_string();
            return;
        };

        let start_index = self.queue.len();
        self.queue.push(song.clone());
        if self.queue_index.is_none() {
            self.set_queue_index(Some(start_index));
            match self.play_current_queue_song() {
                Ok(()) => {
                    self.status = format!(
                        "Queued song and started playback: {} - {}",
                        song.artist_name, song.title
                    );
                }
                Err(err) => {
                    self.status = format!("Queued song, but playback failed: {err}");
                }
            }
        } else {
            self.status = format!("Queued song: {} - {}", song.artist_name, song.title);
        }
    }

    fn enqueue_selected_album(&mut self) {
        let Some(album) = self.browser.selected_album().cloned() else {
            self.status = "Select an album to queue (Albums tab)".to_string();
            return;
        };

        match self.cache.songs_for_album(&self.client, &album.id) {
            Ok(songs) if songs.is_empty() => {
                self.status = format!("No songs found for album: {}", album.title);
            }
            Ok(songs) => {
                let added = songs.len();
                let start_index = self.queue.len();
                self.queue.extend(songs.iter().cloned());
                if self.queue_index.is_none() {
                    self.set_queue_index(Some(start_index));
                    match self.play_current_queue_song() {
                        Ok(()) => {
                            self.status = format!(
                                "Queued album and started playback: {} ({added} songs)",
                                album.title
                            );
                        }
                        Err(err) => {
                            self.status = format!(
                                "Queued album: {} ({added} songs), but playback failed: {err}",
                                album.title
                            );
                        }
                    }
                } else {
                    self.status = format!("Queued album: {} ({added} songs)", album.title);
                }
            }
            Err(err) => {
                self.status = format!("Failed to queue album: {err}");
            }
        }
    }

    fn toggle_pause(&mut self) {
        if self.current_song().is_none() || !self.player.has_active_playback() {
            self.status = "Nothing to play/pause".to_string();
            return;
        }

        match self.player.toggle_pause() {
            Ok(true) => {
                self.pause_playback_clock();
                self.status = "Paused".to_string();
            }
            Ok(false) => {
                self.resume_playback_clock();
                self.status = "Resumed".to_string();
            }
            Err(err) => self.status = format!("Pause failed: {err}"),
        }
    }

    fn shuffle_current_context(&mut self) -> Result<()> {
        let mut songs = match self.browser.active_tab() {
            Tab::Artists => match self.browser.selected_artist().cloned() {
                Some(artist) => self.collect_artist_songs(&artist.id)?,
                None => Vec::new(),
            },
            Tab::Albums => match self.browser.selected_album().cloned() {
                Some(album) => self
                    .cache
                    .songs_for_album(&self.client, &album.id)?
                    .to_vec(),
                None => Vec::new(),
            },
            Tab::Songs => self.cache.all_songs(&self.client)?.to_vec(),
        };

        if songs.is_empty() {
            self.status = "No songs available for shuffle context".to_string();
            return Ok(());
        }

        songs.shuffle(&mut rand::rng());
        self.queue = songs;
        self.set_queue_index(Some(0));
        if let Err(err) = self.play_current_queue_song() {
            self.status = format!("Playback failed: {err}");
        }

        Ok(())
    }

    fn collect_artist_songs(&mut self, artist_id: &str) -> Result<Vec<Song>> {
        let albums = self
            .cache
            .albums_for_artist(&self.client, artist_id)?
            .to_vec();
        let mut songs = Vec::new();
        for album in albums {
            songs.extend(
                self.cache
                    .songs_for_album(&self.client, &album.id)?
                    .iter()
                    .filter(|song| song.has_artist_id(artist_id))
                    .cloned(),
            );
        }
        Ok(songs)
    }

    fn current_song(&self) -> Option<&Song> {
        self.queue_index.and_then(|index| self.queue.get(index))
    }

    fn play_current_queue_song(&mut self) -> Result<()> {
        let Some(song) = self.current_song().cloned() else {
            return Ok(());
        };

        self.play_song_direct(&song)
    }

    fn play_song_direct(&mut self, song: &Song) -> Result<()> {
        self.play_song_direct_with_seek(song, 0.0)
    }

    fn play_song_direct_seek(&mut self, song: &Song, seek_seconds: f64) -> Result<()> {
        let target = self.client.stream_target(&song.id)?;
        if let Err(_first_err) =
            self.player
                .play_target_seek(&target, self.volume_percent, seek_seconds)
        {
            // Retry once in case fast-start mode is not compatible for this track/server.
            self.player
                .play_target_compat_seek(&target, self.volume_percent, seek_seconds)?;
        }
        self.failed_retry_song_id = None;
        self.reset_playback_clock(seek_seconds);
        Ok(())
    }

    fn play_song_direct_with_seek(&mut self, song: &Song, seek_seconds: f64) -> Result<()> {
        let target = self.client.stream_target(&song.id)?;
        if let Err(_first_err) = self
            .player
            .play_target(&target, self.volume_percent, seek_seconds)
        {
            // Retry once in case fast-start mode is not compatible for this track/server.
            self.player
                .play_target_compat(&target, self.volume_percent, seek_seconds)?;
        }
        self.failed_retry_song_id = None;
        self.reset_playback_clock(seek_seconds);
        Ok(())
    }

    fn advance_after_track_end(&mut self, exit_status: std::process::ExitStatus) {
        self.pause_playback_clock();
        let action = decide_track_end_action(
            exit_status.success(),
            self.queue_index,
            self.queue.len(),
            self.current_song().map(|song| song.id.as_str()),
            self.failed_retry_song_id.as_deref(),
        );

        match action {
            TrackEndAction::None => {}
            TrackEndAction::RetryCurrent => {
                let Some(song) = self.current_song().cloned() else {
                    self.status = "Playback failed; queue paused on current item".to_string();
                    return;
                };
                self.failed_retry_song_id = Some(song.id.clone());
                match self.play_song_direct(&song) {
                    Ok(()) => {
                        self.status = format!(
                            "Retrying playback in compatibility mode: {} - {}",
                            song.artist_name, song.title
                        );
                    }
                    Err(err) => {
                        self.status = format!("Playback retry failed: {err}");
                    }
                }
            }
            TrackEndAction::PauseOnFailure => {
                self.status = "Playback failed; queue paused on current item".to_string();
            }
            TrackEndAction::AdvanceTo(next_index) => {
                self.set_queue_index(Some(next_index));
                if let Err(err) = self.play_current_queue_song() {
                    self.status = format!("Auto-advance failed: {err}");
                }
            }
            TrackEndAction::QueueComplete => {
                self.status = "Queue complete".to_string();
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let screen = frame.size();
        let now_height = 5_u16.min(screen.height);
        let content_height = screen.height.saturating_sub(now_height);
        let min_main = 8_u16.min(content_height);
        let min_queue = 6_u16.min(content_height.saturating_sub(min_main));

        // Moderate bias to the browser pane while still letting queue grow on tall terminals.
        let mut main_height = ((u32::from(content_height) * 76) / 100) as u16;
        let max_main = content_height.saturating_sub(min_queue);
        if max_main < min_main {
            main_height = max_main;
        } else {
            main_height = main_height.clamp(min_main, max_main);
        }
        let queue_height = content_height.saturating_sub(main_height);

        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(main_height),
                Constraint::Length(now_height),
                Constraint::Length(queue_height),
            ])
            .split(screen);

        self.volume_slider_hitbox = None;
        self.timeline_slider_hitbox = None;
        self.main_hitbox = Some(areas[0]);
        self.queue_hitbox = Some(areas[2]);
        self.draw_main_pane(frame, areas[0]);
        self.draw_now_playing(frame, areas[1]);
        self.draw_queue(frame, areas[2]);
    }

    fn draw_main_pane(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let base_title = match self.interaction_mode {
            InteractionMode::Queue => " Artists  Albums  Songs",
            InteractionMode::Browser => match self.browser.active_tab() {
                Tab::Artists => "[Artists]  Albums  Songs",
                Tab::Albums => " Artists  [Albums]  Songs",
                Tab::Songs => " Artists  Albums  [Songs]",
            },
        };
        let title = match &self.input_mode {
            InputMode::Search { buffer } => format!("{base_title}  /{buffer}"),
            InputMode::Normal if !self.browser.active_filter().is_empty() => {
                format!("{base_title}  /{}", self.browser.active_filter())
            }
            InputMode::Normal => base_title.to_string(),
        };

        let block = Block::default().title(title).borders(Borders::ALL);
        let mut list_state = ListState::default();
        let (items, mut selected_index) = match self.browser.active_tab() {
            Tab::Artists => (
                self.browser
                    .artists()
                    .iter()
                    .map(|artist| ListItem::new(artist.name.clone()))
                    .collect::<Vec<_>>(),
                if self.browser.artists().is_empty() {
                    None
                } else {
                    Some(self.browser.selected_artist_index())
                },
            ),
            Tab::Albums => (
                self.browser
                    .albums()
                    .iter()
                    .map(|album| ListItem::new(format!("{} — {}", album.artist_name, album.title)))
                    .collect::<Vec<_>>(),
                if self.browser.albums().is_empty() {
                    None
                } else {
                    Some(self.browser.selected_album_index())
                },
            ),
            Tab::Songs => (
                self.browser
                    .songs()
                    .iter()
                    .map(|song| {
                        let track = song.track.unwrap_or(0);
                        ListItem::new(format!("{track:02} {}", song.title))
                    })
                    .collect::<Vec<_>>(),
                if self.browser.songs().is_empty() {
                    None
                } else {
                    Some(self.browser.selected_song_index())
                },
            ),
        };
        if self.interaction_mode == InteractionMode::Queue {
            selected_index = None;
        }

        list_state.select(selected_index);
        let list = List::new(items)
            .block(block)
            .highlight_symbol("> ")
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, area, &mut list_state);
    }

    fn draw_now_playing(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let line1 = if self.player.has_active_playback() {
            if let Some(song) = self.current_song() {
                let state = if self.player.paused() {
                    "Paused"
                } else {
                    "Playing"
                };
                format!("{state}: {} - {}", song.artist_name, song.title)
            } else {
                "Playing".to_string()
            }
        } else {
            "Stopped".to_string()
        };

        let (line2, volume_hitbox) = self.build_volume_slider_line(area);
        let (line3, timeline_hitbox) = self.build_timeline_scrubber_line(area);
        self.volume_slider_hitbox = volume_hitbox;
        self.timeline_slider_hitbox = timeline_hitbox;

        let para = Paragraph::new(format!("{line1}\n{line2}\n{line3}"))
            .block(Block::default().title("Now Playing").borders(Borders::ALL));
        frame.render_widget(para, area);
    }

    fn build_volume_slider_line(
        &self,
        area: ratatui::layout::Rect,
    ) -> (String, Option<VolumeSliderHitbox>) {
        let inner_width = area.width.saturating_sub(2) as usize;
        let prefix = format!("VOL {:>3}% |", self.volume_percent);
        let suffix = "|";
        let fixed_bar = 20usize;
        let max_bar = inner_width.saturating_sub(prefix.len() + suffix.len());
        let bar_width = fixed_bar.min(max_bar);

        if bar_width == 0 || area.height < 4 {
            return (format!("VOL {:>3}%", self.volume_percent), None);
        }

        let filled = ((usize::from(self.volume_percent) * bar_width) + 50) / 100;
        let empty = bar_width.saturating_sub(filled);
        let line = format!(
            "{prefix}{}{empty_part}{suffix}",
            "■".repeat(filled),
            empty_part = "□".repeat(empty),
        );

        let hitbox = VolumeSliderHitbox {
            x: area.x.saturating_add(1).saturating_add(prefix.len() as u16),
            y: area.y.saturating_add(2),
            width: bar_width as u16,
        };

        (line, Some(hitbox))
    }

    fn build_timeline_scrubber_line(
        &self,
        area: ratatui::layout::Rect,
    ) -> (String, Option<TimelineSliderHitbox>) {
        let elapsed = self.playback_elapsed_seconds();
        let elapsed_label = format_timestamp(elapsed);
        let total_seconds = self.current_song().and_then(|song| song.duration_seconds);
        let total_label = total_seconds
            .map(|seconds| format_timestamp(seconds as f64))
            .unwrap_or_else(|| "--:--".to_string());

        let inner_width = area.width.saturating_sub(2) as usize;
        let prefix = format!("POS {elapsed_label} |");
        let suffix = format!("| {total_label}");
        let fixed_bar = 30usize;
        let max_bar = inner_width.saturating_sub(prefix.len() + suffix.len());
        let bar_width = fixed_bar.min(max_bar);
        if bar_width == 0 || area.height < 5 {
            return (format!("POS {elapsed_label} / {total_label}"), None);
        }

        let played_ratio = match total_seconds {
            Some(total) if total > 0 => (elapsed / total as f64).clamp(0.0, 1.0),
            _ => 0.0,
        };
        let filled = ((played_ratio * bar_width as f64) + 0.5) as usize;
        let filled = filled.min(bar_width);
        let empty = bar_width.saturating_sub(filled);
        let line = format!(
            "{prefix}{}{empty_part}{suffix}",
            "■".repeat(filled),
            empty_part = "□".repeat(empty),
        );
        let hitbox = TimelineSliderHitbox {
            x: area.x.saturating_add(1).saturating_add(prefix.len() as u16),
            y: area.y.saturating_add(3),
            width: bar_width as u16,
        };
        (line, Some(hitbox))
    }

    fn draw_queue(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        self.queue_visible_rows = area.height.saturating_sub(2) as usize;
        if self.queue.is_empty() {
            self.queue_nav_index = None;
            self.queue_reorder_index = None;
        } else if let Some(nav_index) = self.queue_nav_index
            && nav_index >= self.queue.len()
        {
            self.queue_nav_index = Some(self.queue.len().saturating_sub(1));
        }
        if let Some(reorder_index) = self.queue_reorder_index
            && reorder_index >= self.queue.len()
        {
            self.queue_reorder_index = Some(self.queue.len().saturating_sub(1));
        }
        if let Some(index) = self.queue_follow_index.take() {
            self.ensure_queue_row_visible(index);
        }
        let max_offset = self.max_queue_scroll_offset();
        if self.queue_scroll_offset > max_offset {
            self.queue_scroll_offset = max_offset;
        }

        let start = self.queue_scroll_offset;
        let end = if self.queue_visible_rows == 0 {
            start
        } else {
            start
                .saturating_add(self.queue_visible_rows)
                .min(self.queue.len())
        };

        let queue_title = match self.interaction_mode {
            InteractionMode::Browser => "Queue",
            InteractionMode::Queue => "[Queue]",
        };
        let title = format!("{queue_title} | {}", self.status);
        let items = self
            .queue
            .iter()
            .enumerate()
            .skip(start)
            .take(end.saturating_sub(start))
            .map(|(index, song)| {
                let marker = if self.interaction_mode == InteractionMode::Queue {
                    if Some(index) == self.queue_reorder_index && Some(index) == self.queue_index {
                        "M*"
                    } else if Some(index) == self.queue_reorder_index {
                        "M>"
                    } else if Some(index) == self.queue_index && Some(index) == self.queue_nav_index
                    {
                        ">>"
                    } else if Some(index) == self.queue_nav_index {
                        "> "
                    } else if Some(index) == self.queue_index {
                        "* "
                    } else {
                        "  "
                    }
                } else if Some(index) == self.queue_index {
                    ">>"
                } else {
                    "  "
                };
                ListItem::new(format!("{marker} {} - {}", song.artist_name, song.title))
            })
            .collect::<Vec<_>>();

        let selected = if self.interaction_mode == InteractionMode::Queue {
            self.queue_nav_index
                .filter(|index| *index >= start && *index < end)
                .map(|index| index - start)
        } else {
            None
        };

        let mut list_state = ListState::default();
        list_state.select(selected);
        let mut queue_block = Block::default().title(title).borders(Borders::ALL);
        if self.show_identity_label && !self.user_server_label.is_empty() {
            queue_block = queue_block.title(
                Title::from(self.user_server_label.as_str())
                    .position(Position::Bottom)
                    .alignment(Alignment::Left),
            );
        }
        let list = List::new(items)
            .block(queue_block)
            .highlight_symbol("> ")
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, area, &mut list_state);
    }
}

fn warm_library_in_background(
    client: SubsonicClient,
    artist_ids: Vec<String>,
    loaded_artist_ids: HashSet<String>,
    loaded_album_ids: HashSet<String>,
    tx: mpsc::Sender<LibraryWarmupEvent>,
) {
    let mut missing_artist_ids: Vec<String> = artist_ids
        .into_iter()
        .filter(|artist_id| !loaded_artist_ids.contains(artist_id))
        .collect();
    missing_artist_ids.sort_unstable();

    let mut known_album_ids = loaded_album_ids.clone();
    let artist_total = missing_artist_ids.len();
    for (index, artist_id) in missing_artist_ids.into_iter().enumerate() {
        let albums = match client.get_albums_by_artist(&artist_id) {
            Ok(albums) => albums,
            Err(err) => {
                let _ = tx.send(LibraryWarmupEvent::Failed(err.to_string()));
                return;
            }
        };
        for album in &albums {
            known_album_ids.insert(album.id.clone());
        }
        if tx
            .send(LibraryWarmupEvent::ArtistAlbums {
                artist_id,
                albums,
                done: index + 1,
                total: artist_total,
            })
            .is_err()
        {
            return;
        }
    }

    let mut missing_album_ids: Vec<String> = known_album_ids
        .into_iter()
        .filter(|album_id| !loaded_album_ids.contains(album_id))
        .collect();
    missing_album_ids.sort_unstable();

    let album_total = missing_album_ids.len();
    for (index, album_id) in missing_album_ids.into_iter().enumerate() {
        let songs = match client.get_songs_by_album(&album_id) {
            Ok(songs) => songs,
            Err(err) => {
                let _ = tx.send(LibraryWarmupEvent::Failed(err.to_string()));
                return;
            }
        };
        if tx
            .send(LibraryWarmupEvent::AlbumSongs {
                album_id,
                songs,
                done: index + 1,
                total: album_total,
            })
            .is_err()
        {
            return;
        }
    }

    let _ = tx.send(LibraryWarmupEvent::Done);
}

fn user_server_label(client: &SubsonicClient) -> String {
    let username = client.username().trim();
    let host = reqwest::Url::parse(client.server_url())
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| fallback_server_host(client.server_url()));

    if username.is_empty() {
        host
    } else if host.is_empty() {
        username.to_string()
    } else {
        format!("{username}@{host}")
    }
}

fn fallback_server_host(server_url: &str) -> String {
    server_url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .split('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

fn format_timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0).floor() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

fn decide_track_end_action(
    successful_exit: bool,
    queue_index: Option<usize>,
    queue_len: usize,
    current_song_id: Option<&str>,
    failed_retry_song_id: Option<&str>,
) -> TrackEndAction {
    if !successful_exit {
        if let Some(song_id) = current_song_id
            && failed_retry_song_id != Some(song_id)
        {
            return TrackEndAction::RetryCurrent;
        }
        return TrackEndAction::PauseOnFailure;
    }

    match queue_index {
        Some(index) if index + 1 < queue_len => TrackEndAction::AdvanceTo(index + 1),
        Some(_) => TrackEndAction::QueueComplete,
        None => TrackEndAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KeybindsConfig;
    use crate::library::LibraryCache;
    use crate::model::Artist;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn test_app() -> App {
        let client = SubsonicClient::new("http://127.0.0.1:4533", "tester", "secret")
            .expect("test client should construct");
        let artists = vec![
            Artist {
                id: "a1".to_string(),
                name: "Alpha".to_string(),
            },
            Artist {
                id: "a2".to_string(),
                name: "Beta".to_string(),
            },
        ];
        App::new(
            client,
            LibraryCache::new(artists),
            false,
            true,
            KeybindsConfig::default(),
        )
        .expect("app should initialize")
    }

    fn queue_song(id: &str, title: &str) -> Song {
        Song {
            id: id.to_string(),
            album_id: "alb".to_string(),
            album_title: "Album".to_string(),
            artist_id: "art".to_string(),
            artist_ids: vec!["art".to_string()],
            artist_name: "Artist".to_string(),
            title: title.to_string(),
            duration_seconds: Some(180),
            track: None,
        }
    }

    #[test]
    fn queue_cursor_wraps_in_both_directions() {
        let mut app = test_app();
        app.queue = vec![
            queue_song("1", "One"),
            queue_song("2", "Two"),
            queue_song("3", "Three"),
        ];
        app.queue_nav_index = Some(0);

        app.queue_move_cursor(-1);
        assert_eq!(app.queue_nav_index, Some(2));

        app.queue_move_cursor(1);
        assert_eq!(app.queue_nav_index, Some(0));
    }

    #[test]
    fn queue_reorder_cancel_restores_original_order() {
        let mut app = test_app();
        let original = vec![
            queue_song("1", "One"),
            queue_song("2", "Two"),
            queue_song("3", "Three"),
        ];
        app.queue = original.clone();
        app.queue_nav_index = Some(0);
        app.queue_index = Some(1);

        app.toggle_queue_reorder_mode();
        app.queue_reorder_move(1);
        assert_ne!(app.queue, original);

        app.cancel_queue_reorder_mode();
        assert_eq!(app.queue, original);
        assert_eq!(app.queue_reorder_index, None);
        assert_eq!(app.queue_nav_index, Some(0));
        assert_eq!(app.queue_index, Some(1));
    }

    #[test]
    fn search_mode_treats_digit_hotkeys_as_text_input() {
        let mut app = test_app();
        app.begin_search();

        app.on_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));

        assert_eq!(app.browser.active_tab(), Tab::Artists);
        match &app.input_mode {
            InputMode::Search { buffer } => assert_eq!(buffer, "2"),
            InputMode::Normal => panic!("expected to remain in search mode"),
        }
    }

    #[test]
    fn backspace_on_empty_search_exits_search_mode() {
        let mut app = test_app();
        app.begin_search();

        app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));

        assert!(matches!(app.input_mode, InputMode::Normal));
    }

    #[test]
    fn track_end_decision_retries_once_after_failure() {
        let action = decide_track_end_action(false, Some(0), 2, Some("song-1"), None);
        assert_eq!(action, TrackEndAction::RetryCurrent);

        let action = decide_track_end_action(false, Some(0), 2, Some("song-1"), Some("song-1"));
        assert_eq!(action, TrackEndAction::PauseOnFailure);
    }

    #[test]
    fn track_end_decision_advances_or_completes_on_success() {
        assert_eq!(
            decide_track_end_action(true, Some(0), 2, Some("song-1"), None),
            TrackEndAction::AdvanceTo(1)
        );
        assert_eq!(
            decide_track_end_action(true, Some(1), 2, Some("song-2"), None),
            TrackEndAction::QueueComplete
        );
        assert_eq!(
            decide_track_end_action(true, None, 2, None, None),
            TrackEndAction::None
        );
    }
}
