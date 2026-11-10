//! The game's window: it runs the game, the computer players and the interface, and draws them.

mod widgets;
mod window;

use std::hash::{BuildHasher, Hasher};
use std::time::Instant;

use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;

use crate::Result;
use crate::board_geometry::Layout;
use crate::board_view::{BoardView, Event};
use crate::camera::OrbitCamera;
use crate::engine::{self, Ownership, SearchLimits, SearchResult, Worker};
use crate::gfx;
use crate::go::{self, Cell, Color, Game, MAX_SIZE, PASS, POINT_COUNT, Phase, Point};
use crate::math::{Mat4, Vec3, vec3};
use crate::platform::GlWindow;
use crate::renderer::{FrostedPanel, Renderer, Scene, StoneDraw, mark};
use crate::settings::{Settings, WindowPlacement};
use crate::sound::{Effect, SoundBank};
use crate::stone_model::{self, HALF_HEIGHT};
use crate::strings::{Language, Str, fill, tr};
use crate::text::FontLibrary;
use crate::themes::{THEME_COUNT, themes};
use crate::ui::{self, FontId, Painter, Rect, rect, rgb, rgba};

pub use window::run;

use widgets::{MUTED_COLOR, TEXT_COLOR, Toast};
use window::{Key, Pointer};

const TITLE: &str = "3D Go";
// The window's size where none is remembered, and its smallest, on a screen of the usual scale.
const DEFAULT_SIZE: (f64, f64) = (1500.0, 920.0);
const MIN_SIZE: (f64, f64) = (900.0, 680.0);

// The widgets; a selector takes two numbers, one for each of its arrows.
const HELP_BUTTON: i32 = 1;
const PASS_BUTTON: i32 = 2;
const RESIGN_BUTTON: i32 = 3;
const ESTIMATE_BUTTON: i32 = 4;
const ACCEPT_BUTTON: i32 = 5;
const RESUME_BUTTON: i32 = 6;
const UNDO_BUTTON: i32 = 7;
const REDO_BUTTON: i32 = 8;
const HINT_BUTTON: i32 = 9;
const NEW_GAME_BUTTON: i32 = 10;
const FLIP_BUTTON: i32 = 11;
const MODE_SELECTOR: i32 = 12;
const LEVEL_SELECTOR: i32 = 14;
const BOARD_SELECTOR: i32 = 16;
const HANDICAP_SELECTOR: i32 = 18;
const THEME_SELECTOR: i32 = 20;
const SOUND_BUTTON: i32 = 22;
const COORDINATES_BUTTON: i32 = 23;
const LANGUAGE_BUTTON: i32 = 24;
const HELP_CLOSE: i32 = 101;

const BLACK_STONE: ui::Color = rgb(22, 22, 26);
const WHITE_STONE: ui::Color = rgb(238, 236, 230);
const STONE_RIM: ui::Color = rgb(120, 125, 140);
const BOARD_SIZES: [usize; 3] = [9, 13, 19];

const HELP_ROWS: usize = 12; // in each of the two columns
const HELP_WIDTH: f32 = 880.0;
const HELP_ROW: f32 = 26.0;
const HELP_PAD: f32 = 28.0;

/// The keys of the help, each with what it does: a column of the mouse and the game, a column of the view and
/// the settings.
const HELP_LINES: [(Option<Str>, &str, Str); 2 * HELP_ROWS] = [
    (Some(Str::KeyClick), "", Str::HelpClick),
    (Some(Str::KeyDrag), "", Str::HelpRotate),
    (Some(Str::KeyWheel), "", Str::HelpZoom),
    (None, "Ctrl+Z  ←  Backspace", Str::HelpUndo),
    (None, "Ctrl+Y  →", Str::HelpRedo),
    (None, "P", Str::HelpPass),
    (None, "H", Str::HelpHint),
    (None, "E", Str::HelpEstimate),
    (None, "Enter", Str::HelpAccept),
    (None, "N", Str::HelpNew),
    (None, "Ctrl+C", Str::HelpCopy),
    (None, "Ctrl+V", Str::HelpPaste),
    (None, "F", Str::HelpFlip),
    (None, "V", Str::HelpTopView),
    (None, "1 – 5", Str::HelpLevel),
    (None, "M", Str::HelpMode),
    (None, "B", Str::HelpBoard),
    (None, "C", Str::HelpCoordinates),
    (None, "T", Str::HelpTheme),
    (None, "S", Str::HelpSound),
    (None, "L", Str::HelpLanguage),
    (None, "Tab", Str::HelpPanel),
    (None, "F11  Alt+Enter", Str::HelpFullScreen),
    (None, "Esc  F1", Str::HelpClose),
];

/// Who plays whom.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    PlayBlack,
    PlayWhite,
    TwoPlayers,
    Demo,
}

impl Mode {
    const ALL: [Mode; 4] = [Mode::PlayBlack, Mode::PlayWhite, Mode::TwoPlayers, Mode::Demo];

    /// The mode a number of steps on, round and round.
    fn stepped(self, steps: i32) -> Mode {
        Mode::ALL[(self as i32 + steps).rem_euclid(4) as usize]
    }

    fn against_computer(self) -> bool {
        matches!(self, Mode::PlayBlack | Mode::PlayWhite)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Gesture {
    None,
    PressPoint,
    TurnCamera,
}

/// The threads a search may use: all but one, which is left for the window.
fn threads() -> usize {
    std::thread::available_parallelism().map_or(1, |count| count.get() - 1).clamp(1, 8)
}

/// The playouts, the time and the threads of each level.
fn level_limits(level: i32, seed: u64) -> SearchLimits {
    let threads = threads();
    let (playouts, milliseconds, threads) = match level {
        1 => (300, 1000, 1),
        2 => (1500, 1500, threads.min(2)),
        3 => (6000, 2000, threads),
        4 => (30_000, 2500, threads),
        _ => (200_000, 6000, threads),
    };
    SearchLimits { playouts, milliseconds, threads, may_resign: true, seed }
}

/// Playouts for an estimate of the score: enough to be steady, and quick on the big board too.
fn estimate_playouts(size: usize) -> usize {
    match size {
        ..=9 => 4000,
        10..=13 => 3000,
        _ => 2400,
    }
}

/// A number that differs from run to run and from call to call.
fn random_seed() -> u64 {
    std::collections::hash_map::RandomState::new().build_hasher().finish()
}

/// A little stone for the panel, with a rim and a gleam.
fn draw_stone(painter: &mut Painter, x: f32, y: f32, radius: f32, side: Color) {
    let (color, gleam) = if side == Color::Black { (BLACK_STONE, 46) } else { (WHITE_STONE, 120) };
    painter.disc(x, y, radius, STONE_RIM);
    painter.disc(x, y, radius - (radius * 0.1).max(1.0), color);
    painter.disc(x - radius * 0.28, y - radius * 0.3, radius * 0.38, rgba(255, 255, 255, gleam));
}

/// Where a point's marks are in the scene.
fn mark_index(point: Point) -> usize {
    go::x_of(point) + MAX_SIZE * go::y_of(point)
}

pub struct App {
    renderer: Renderer,
    painter: Painter,
    fonts: FontLibrary,
    view: BoardView,
    sound: SoundBank,
    clipboard: Option<arboard::Clipboard>,
    settings: Settings,
    language: Language,
    width: i32,
    height: i32,
    scale: f32,      // of the interface: 1 at 96 dots per inch
    font_scale: f32, // the scale the fonts were made for
    dirty: bool,     // something to draw since the last frame
    idle: bool,      // waiting for input: the time waited is not animation time
    full_screen: bool,
    maximized: bool,
    placement: Option<WindowPlacement>,
    last_frame: Instant,
    time: f32,

    game: Game,
    layout: Layout,
    camera: OrbitCamera,
    mode: Mode,
    level: i32,
    theme: usize,
    // For new games.
    board_size: usize,
    handicap: usize,
    coordinates: bool,
    computer: Worker<SearchResult>,
    computer_thinking: bool,
    computer_clock: f32,
    computer_result: Option<SearchResult>,
    hinter: Worker<SearchResult>,
    hint_thinking: bool,
    hint: Option<Point>,
    hint_glow: f32,
    estimator: Worker<Ownership>,
    estimating: bool,
    estimate: Option<Ownership>,    // shown over the board until the position changes
    dead_finder: Worker<Ownership>, // at the start of the counting
    finding_dead: bool,
    dead_found_ply: Option<usize>,
    scoring_clock: f32,
    resign_confirm: f32, // while above zero, a second click on Resign resigns
    announced_end: bool,
    events: Vec<Event>,

    pointer: Pointer,
    modifiers: ModifiersState,
    gesture: Gesture,
    press_x: f32,
    press_y: f32,
    // The pointer when the camera last turned.
    drag_x: f32,
    drag_y: f32,
    press_point: Option<Point>,
    hover_point: Option<Point>, // a legal point under the pointer, or a stone while counting
    cursor_hand: bool,
    shown_cursor_hand: bool,

    active_widget: i32,
    panel_visible: bool,
    panel_slide: f32,
    help_visible: bool,
    move_scroll: f32,
    follow_moves: bool,
    banner_fade: f32,
    // Kept while the banner fades after a new game.
    banner_title: String,
    banner_detail: String,
    toasts: Vec<Toast>,

    // The window goes last: what was made in its OpenGL context is dropped before it.
    gl: GlWindow,
}

impl App {
    fn new(event_loop: &ActiveEventLoop) -> Result<App> {
        let settings = Settings::load();
        let gl = window::open(event_loop, settings.window, settings.maximized)?;
        gfx::set_glsl_version(gl.version());

        let mut painter = Painter::new()?;
        let mut fonts = FontLibrary::new();
        let scale = gl.window.scale_factor() as f32;
        painter.set_fonts(widgets::build_fonts(&mut fonts, scale)?);
        gl.window.set_visible(true);
        let size = gl.window.inner_size();
        let (width, height) = (size.width as i32, size.height as i32);
        let language = settings.language;
        widgets::show_loading(&gl, &mut painter, tr(Str::Title, language));

        let game = Game::from_sgf(&settings.game).unwrap_or_else(|_| Game::new(settings.board_size, settings.handicap));
        let layout = Layout::new(game.size());
        let theme = settings.theme as usize;
        let renderer = Renderer::new(&stone_model::build_stone(), &mut fonts, &themes()[theme], &layout)?;
        let mut view = BoardView::new(&layout);
        view.rebuild(&game, true);
        let mut sound = SoundBank::new();
        sound.enabled = settings.sound;

        let mut app = App {
            renderer,
            painter,
            fonts,
            view,
            sound,
            clipboard: None,
            language,
            width,
            height,
            scale,
            font_scale: scale,
            dirty: true,
            idle: false,
            full_screen: false,
            maximized: settings.maximized,
            placement: settings.window,
            last_frame: Instant::now(),
            time: 0.0,
            announced_end: game.phase() == Phase::Finished,
            game,
            camera: OrbitCamera::new(layout.extent()),
            layout,
            mode: Mode::ALL[settings.mode as usize],
            level: settings.level,
            theme,
            board_size: settings.board_size,
            handicap: settings.handicap,
            coordinates: settings.coordinates,
            computer: Worker::default(),
            computer_thinking: false,
            computer_clock: 0.0,
            computer_result: None,
            hinter: Worker::default(),
            hint_thinking: false,
            hint: None,
            hint_glow: 0.0,
            estimator: Worker::default(),
            estimating: false,
            estimate: None,
            dead_finder: Worker::default(),
            finding_dead: false,
            dead_found_ply: None,
            scoring_clock: 0.0,
            resign_confirm: 0.0,
            events: Vec::new(),
            pointer: Pointer::default(),
            modifiers: ModifiersState::empty(),
            gesture: Gesture::None,
            press_x: 0.0,
            press_y: 0.0,
            drag_x: 0.0,
            drag_y: 0.0,
            press_point: None,
            hover_point: None,
            cursor_hand: false,
            shown_cursor_hand: false,
            active_widget: 0,
            panel_visible: settings.panel,
            panel_slide: if settings.panel { 1.0 } else { 0.0 },
            help_visible: false,
            move_scroll: 0.0,
            follow_moves: true,
            banner_fade: 0.0,
            banner_title: String::new(),
            banner_detail: String::new(),
            toasts: Vec::new(),
            settings,
            gl,
        };
        app.camera.reset(app.view_side());
        app.track_placement();
        Ok(app)
    }

    fn frame(&mut self) -> Result<()> {
        let Some(seconds) = self.begin_frame()? else { return Ok(()) };
        self.renderer.resize(self.width, self.height)?;

        self.update(seconds);
        let scene = self.build_scene();
        self.renderer.render(&scene);

        self.cursor_hand = false;
        self.painter.begin(self.width, self.height);
        self.draw_interface();
        self.painter.flush();
        self.handle_board_pointer();

        self.end_frame();
        Ok(())
    }

    fn build_scene(&self) -> Scene {
        let board = self.game.current();
        let phase = self.phase();
        let mut marks = [0u8; MAX_SIZE * MAX_SIZE];

        // The territory, as counted at the end or estimated on request; the stones that are dead in it are seen through.
        let mut see_through = None;
        if self.counted() {
            let owner = self.game.territory();
            let dead = self.game.dead();
            for point in board.points().filter(|point| !board.is_stone(*point) || dead[*point]) {
                match owner[point] {
                    Cell::Black => marks[mark_index(point)] |= mark::BLACK_AREA,
                    Cell::White => marks[mark_index(point)] |= mark::WHITE_AREA,
                    _ => {}
                }
            }
            see_through = Some(*dead);
        } else if let Some(estimate) = &self.estimate {
            let mut lost = [false; POINT_COUNT];
            for point in board.points() {
                let (share, cell) = (estimate.black[point], board.at(point));
                if share > 0.7 && cell != Cell::Black {
                    marks[mark_index(point)] |= mark::BLACK_AREA;
                    lost[point] = cell == Cell::White;
                } else if share < 0.3 && cell != Cell::White {
                    marks[mark_index(point)] |= mark::WHITE_AREA;
                    lost[point] = cell == Cell::Black;
                }
            }
            see_through = Some(lost);
        }

        // The stone played last carries a ring; after a pass there is none.
        let last_move = self.game.moves().last().filter(|last| !last.is_pass() && !last.is_resignation()).map(|last| last.point);
        let mut stones = Vec::new();
        self.view.draws(&mut stones, last_move, see_through.as_ref(), self.theme().accent_warm, self.time);
        self.view.mark_stones(&mut marks);

        // The stone that a click would place, seen through; while counting, the group under the pointer.
        let ready = phase == Phase::Playing && self.human_to_move() && self.view.board_ready();
        if let Some(hover) = self.hover_point {
            if ready {
                stones.push(StoneDraw {
                    model: Mat4::translation(self.layout.point_center(hover) + vec3(0.0, HALF_HEIGHT + 0.015, 0.0)) * Mat4::rotation_y(0.7),
                    white: board.to_play() == Color::White,
                    ground: 0.0,
                    glow_color: Vec3::ZERO,
                    glow: 0.0,
                    dissolve: 0.0,
                    seed: 0.37,
                    mark: 0.0,
                    opacity: 0.5,
                });
            }
            if phase == Phase::Scoring {
                marks[mark_index(hover)] |= mark::HOVER;
            }
        }
        if let Some(hint) = self.hint.filter(|_| self.hint_glow > 0.01) {
            marks[mark_index(hint)] |= mark::HINT;
        }
        if ready && board.ko() != PASS {
            marks[mark_index(board.ko())] |= mark::KO;
        }

        Scene {
            view: self.camera.view(),
            projection: self.camera.projection(self.width as f32 / self.height as f32),
            eye: self.camera.eye(),
            time: self.time,
            stones,
            marks,
            estimate: !self.counted() && self.estimate.is_some(),
            coordinates: self.coordinates,
            panels: self.frosted_panels(),
        }
    }

    fn update(&mut self, seconds: f32) {
        let space = self.slide_panel(seconds);
        self.camera.set_space(space);
        self.camera.update(seconds);

        // The stones that the move under the pointer would take glow and rise, about to go.
        let mut taking = Vec::new();
        if let Some(hover) = self.hover_point.filter(|_| self.human_to_move() && self.view.board_ready()) {
            let mut next = self.game.current().clone();
            next.play(hover, next.to_play(), Some(&mut taking));
        }
        self.view.preview(&taking);

        // One sound of each kind a frame: the stones taken land together.
        self.events.clear();
        self.view.update(seconds, &mut self.events);
        self.events.sort_unstable();
        self.events.dedup();
        for event in &self.events {
            self.sound.play(match event {
                Event::Place => Effect::Place,
                Event::Capture => Effect::Capture,
                Event::Return => Effect::Return,
            });
        }

        let over = self.banner_wanted();
        if over {
            self.banner_title = self.result_title().to_owned();
            self.banner_detail = self.result_detail();
        }
        self.banner_fade = (self.banner_fade + if over { seconds } else { -seconds } * 3.0).clamp(0.0, 1.0);
        self.hint_glow = (self.hint_glow + if self.hint.is_some() { seconds } else { -seconds } * 4.0).clamp(0.0, 1.0);
        self.resign_confirm = (self.resign_confirm - seconds).max(0.0);
        self.age_toasts(seconds);

        self.update_helpers();
        self.update_computer(seconds);
        self.update_scoring(seconds);

        if over && !self.announced_end {
            self.announced_end = true;
            self.sound.play(Effect::GameOver);
        }
        if self.phase() != Phase::Finished {
            self.announced_end = false;
        }
    }

    /// Whether the banner of the game's end is to be seen: the game is over, and the last stones have come to rest.
    fn banner_wanted(&self) -> bool {
        self.phase() == Phase::Finished && self.view.settled()
    }

    fn needs_animation(&self) -> bool {
        let phase = self.phase();
        // What slides and fades is held against where it is going: it may be at rest when its goal changes.
        let panel_target = if self.panel_visible { 1.0 } else { 0.0 };
        let banner_target = if self.banner_wanted() { 1.0 } else { 0.0 };
        self.camera.moving()
            || self.view.animating()
            || self.computer_thinking
            || self.computer_result.is_some()
            || self.hint_thinking
            || self.estimating
            || self.finding_dead
            || !self.toasts.is_empty()
            || self.panel_slide != panel_target
            || self.banner_fade != banner_target
            || self.hint.is_some()
            || self.hint_glow > 0.0
            || self.resign_confirm > 0.0
            || self.gesture != Gesture::None
            || self.pointer.left_down
            || self.pointer.right_down
            || (phase == Phase::Playing && !self.is_human(self.game.current().to_play()))
            || (phase == Phase::Scoring && self.mode == Mode::Demo)
    }

    // The game ---------------------------------------------------------------------------------------

    fn phase(&self) -> Phase {
        self.game.phase()
    }

    /// Whether the points are counted: the game is being scored, or was, and did not end by resignation.
    fn counted(&self) -> bool {
        match self.phase() {
            Phase::Playing => false,
            Phase::Scoring => true,
            Phase::Finished => !self.game.resigned(),
        }
    }

    fn is_human(&self, side: Color) -> bool {
        match self.mode {
            Mode::PlayBlack => side == Color::Black,
            Mode::PlayWhite => side == Color::White,
            Mode::TwoPlayers => true,
            Mode::Demo => false,
        }
    }

    fn human_to_move(&self) -> bool {
        self.phase() == Phase::Playing && self.is_human(self.game.current().to_play())
    }

    /// The side the board is seen from.
    fn view_side(&self) -> Color {
        if self.mode == Mode::PlayWhite { Color::White } else { Color::Black }
    }

    fn stop_workers(&mut self) {
        self.computer.cancel();
        self.computer_thinking = false;
        self.computer_result = None;
        self.hinter.cancel();
        self.hint_thinking = false;
        self.estimator.cancel();
        self.estimating = false;
        self.dead_finder.cancel();
        self.finding_dead = false;
    }

    fn update_computer(&mut self, seconds: f32) {
        let to_play = self.game.current().to_play();
        if self.phase() != Phase::Playing || self.is_human(to_play) {
            return;
        }
        self.computer_clock += seconds;
        if !self.computer_thinking && self.computer_result.is_none() {
            let (board, history, komi) = (self.game.current().clone(), self.game.history().to_vec(), self.game.komi());
            let limits = level_limits(self.level, random_seed());
            self.computer.start(move |stop| engine::search(&board, &history, komi, &limits, stop));
            self.computer_thinking = true;
            self.computer_clock = 0.0;
        }
        if self.computer_thinking {
            let busy = self.computer.busy();
            if let Some(result) = self.computer.take() {
                self.computer_result = Some(result);
                self.computer_thinking = false;
            } else if !busy {
                self.computer_thinking = false; // cancelled; a new search starts next frame
            }
        }
        // The computer waits for the stones to settle, and a moment more, so that its move can be followed.
        let pause = if self.mode == Mode::Demo { 0.6 } else { 0.25 };
        if self.view.settled() && self.computer_clock >= pause && self.gesture == Gesture::None {
            match self.computer_result.take() {
                Some(result) if result.resign => self.resign(to_play),
                Some(result) => self.play_move(result.point),
                None => {}
            }
        }
    }

    /// After two passes the dead stones are found, to be corrected by a click; a demonstration accepts the count
    /// by itself.
    fn update_scoring(&mut self, seconds: f32) {
        if self.phase() != Phase::Scoring {
            return;
        }
        let found = self.dead_found_ply == Some(self.game.ply());
        if !found && !self.finding_dead {
            let (board, komi) = (self.game.current().clone(), self.game.komi());
            let (playouts, seed) = (estimate_playouts(board.size()), random_seed());
            self.dead_finder.start(move |stop| engine::estimate(&board, komi, playouts, seed, stop));
            self.finding_dead = true;
        }
        if self.finding_dead {
            let busy = self.dead_finder.busy();
            if let Some(ownership) = self.dead_finder.take() {
                self.finding_dead = false;
                let dead = ownership.dead(self.game.current());
                self.game.set_dead(&dead);
                self.dead_found_ply = Some(self.game.ply());
                self.scoring_clock = 0.0;
            } else if !busy {
                self.finding_dead = false;
            }
        }
        if self.mode == Mode::Demo && self.dead_found_ply == Some(self.game.ply()) && !self.finding_dead {
            self.scoring_clock += seconds;
            if self.scoring_clock > 1.6 {
                self.accept();
            }
        }
    }

    /// Takes what the hint and the estimate have found.
    fn update_helpers(&mut self) {
        if self.hint_thinking {
            let busy = self.hinter.busy();
            if let Some(result) = self.hinter.take() {
                self.hint_thinking = false;
                if self.human_to_move() {
                    if result.point == PASS {
                        self.show_toast(self.t(Str::HintPass));
                    } else {
                        self.hint = Some(result.point);
                        self.show_toast(format!("{}{}", self.t(Str::HintToast), go::point_name(result.point, self.game.size())));
                    }
                }
            } else if !busy {
                self.hint_thinking = false;
            }
        }
        if self.estimating {
            let busy = self.estimator.busy();
            if let Some(ownership) = self.estimator.take() {
                self.estimating = false;
                let leader = self.t(if ownership.margin >= 0.0 { Str::Black } else { Str::White });
                self.show_toast(fill(self.t(Str::EstimateToast), &[&leader, &format!("{:.1}", ownership.margin.abs())]));
                self.estimate = Some(ownership);
            } else if !busy {
                self.estimating = false;
            }
        }
    }

    fn new_game(&mut self) {
        self.start_game(Game::new(self.board_size, self.handicap));
    }

    /// Sets out a game, with the board of its size.
    fn start_game(&mut self, game: Game) {
        self.stop_workers();
        self.game = game;
        if self.game.size() != self.layout.size {
            self.layout = Layout::new(self.game.size());
            self.renderer.set_layout(&self.layout, &mut self.fonts);
            self.view.set_layout(&self.layout);
            self.camera.set_reach(self.layout.extent());
        }
        self.view.rebuild(&self.game, true);
        self.camera.reset(self.view_side());
        self.announced_end = self.phase() == Phase::Finished;
        self.dead_found_ply = None;
        self.after_position_change();
    }

    fn play_move(&mut self, point: Point) {
        if !self.game.play(point) {
            return;
        }
        if let Some(record) = self.game.moves().last() {
            self.view.play_move(record, true);
        }
        if point == PASS {
            self.announce_pass();
        }
        self.after_position_change();
    }

    /// A toast and a bell for a pass, or the start of the counting after two.
    fn announce_pass(&mut self) {
        let Some(last) = self.game.moves().last().filter(|last| last.is_pass()) else { return };
        let news = match (self.phase(), last.color) {
            (Phase::Scoring, _) => Str::BothPassed,
            (_, Color::Black) => Str::BlackPasses,
            (_, Color::White) => Str::WhitePasses,
        };
        self.show_toast(self.t(news));
        self.sound.play(Effect::Pass);
    }

    fn resign(&mut self, who: Color) {
        if self.phase() == Phase::Finished {
            return;
        }
        self.stop_workers();
        self.game.resign(who);
        self.after_position_change();
    }

    /// Ends the counting with the dead stones as they are marked.
    fn accept(&mut self) {
        if self.phase() != Phase::Scoring || self.finding_dead {
            return;
        }
        self.dead_finder.cancel();
        self.game.accept();
        self.hover_point = None;
        self.save();
    }

    fn undo_one(&mut self) {
        let Some(record) = self.game.moves().last().cloned() else { return };
        self.game.undo();
        self.view.play_move(&record, false);
    }

    fn redo_one(&mut self) {
        if self.game.redo() {
            if let Some(record) = self.game.moves().last() {
                self.view.play_move(record, true);
            }
        }
    }

    fn undo(&mut self) {
        self.stop_workers();
        if !self.game.can_undo() {
            self.show_toast(self.t(Str::NothingToUndo));
            return;
        }
        self.undo_one();
        // Against the computer, back to the player's own turn.
        while self.mode.against_computer() && self.game.can_undo() && !self.human_to_move() {
            self.undo_one();
        }
        self.after_position_change();
    }

    fn redo(&mut self) {
        self.stop_workers();
        if !self.game.can_redo() {
            return;
        }
        self.redo_one();
        while self.mode.against_computer() && self.game.can_redo() && self.phase() == Phase::Playing && !self.human_to_move() {
            self.redo_one();
        }
        self.after_position_change();
    }

    fn set_mode(&mut self, mode: Mode) {
        self.stop_workers();
        self.mode = mode;
        if mode.against_computer() {
            self.camera.face_side(self.view_side());
        }
        self.hint = None;
        self.save();
    }

    fn set_level(&mut self, level: i32) {
        self.level = level.clamp(1, 5);
        if self.computer_thinking {
            self.computer.cancel(); // starts again at the new level
            self.computer_thinking = false;
        }
        self.save();
    }

    /// The size and the handicap are for the next game; a game not yet begun or already over is set out again
    /// at once.
    fn apply_to_next_game(&mut self) {
        if self.game.ply() == 0 || self.phase() == Phase::Finished {
            self.new_game();
        } else {
            self.show_toast(self.t(Str::NextGame));
        }
        self.save();
    }

    fn set_board_size(&mut self, size: usize) {
        self.board_size = size;
        self.handicap = self.handicap.min(go::max_handicap(size));
        self.apply_to_next_game();
    }

    /// The board size a number of steps on among the sizes offered: round and round, or stopping at the ends.
    fn stepped_board_size(&self, steps: i32, wrap: bool) -> usize {
        let count = BOARD_SIZES.len() as i32;
        let at = BOARD_SIZES.iter().position(|size| *size == self.board_size).map_or(count, |at| at as i32);
        BOARD_SIZES[if wrap { (at + steps).rem_euclid(count) } else { (at + steps).clamp(0, count - 1) } as usize]
    }

    /// The handicap a step up or down; there is no handicap of one stone.
    fn step_handicap(&mut self, change: i32) {
        let stones = match self.handicap as i32 + change {
            1 if change > 0 => 2,
            1 => 0,
            stones => stones.clamp(0, go::max_handicap(self.board_size) as i32),
        };
        self.handicap = stones as usize;
        self.apply_to_next_game();
    }

    fn set_theme(&mut self, theme: i32) {
        self.theme = theme.rem_euclid(THEME_COUNT as i32) as usize;
        self.renderer.set_theme(self.theme());
        self.save();
    }

    fn toggle_coordinates(&mut self) {
        self.coordinates = !self.coordinates;
        self.save();
    }

    fn toggle_sound(&mut self) {
        self.sound.enabled = !self.sound.enabled;
        self.save();
    }

    fn toggle_language(&mut self) {
        self.language = self.language.other();
        self.save();
    }

    fn request_hint(&mut self) {
        if !self.human_to_move() || self.hint_thinking {
            return;
        }
        self.hint = None;
        let (board, history, komi) = (self.game.current().clone(), self.game.history().to_vec(), self.game.komi());
        let limits = SearchLimits { playouts: 60_000, milliseconds: 1500, threads: threads(), may_resign: false, seed: random_seed() };
        self.hinter.start(move |stop| engine::search(&board, &history, komi, &limits, stop));
        self.hint_thinking = true;
    }

    /// The territory as it stands, shown until the next move; a second request hides it.
    fn request_estimate(&mut self) {
        if self.phase() != Phase::Playing {
            return;
        }
        if self.estimate.take().is_some() || self.estimating {
            return;
        }
        let (board, komi) = (self.game.current().clone(), self.game.komi());
        let (playouts, seed) = (estimate_playouts(board.size()), random_seed());
        self.estimator.start(move |stop| engine::estimate(&board, komi, playouts, seed, stop));
        self.estimating = true;
    }

    fn after_position_change(&mut self) {
        self.hinter.cancel();
        self.hint_thinking = false;
        self.hint = None;
        self.estimator.cancel();
        self.estimating = false;
        self.estimate = None;
        self.dead_finder.cancel();
        self.finding_dead = false;
        self.resign_confirm = 0.0;
        self.hover_point = None;
        self.follow_moves = true;
        self.save();
    }

    fn copy_game(&mut self) {
        let sgf = self.game.to_sgf(&self.player_name(Color::Black), &self.player_name(Color::White));
        if self.copy_text(if cfg!(windows) { sgf.replace('\n', "\r\n") } else { sgf }) {
            self.show_toast(self.t(Str::GameCopied));
        }
    }

    fn paste_game(&mut self) {
        let Ok(game) = Game::from_sgf(&self.pasted_text()) else {
            self.show_toast(self.t(Str::NothingToPaste));
            return;
        };
        self.board_size = game.size(); // new games go on with the size of the one loaded
        self.handicap = self.handicap.min(go::max_handicap(self.board_size));
        self.start_game(game);
        self.show_toast(self.t(Str::GameLoaded));
    }

    fn save(&mut self) {
        self.settings.theme = self.theme as i32;
        self.settings.level = self.level;
        self.settings.mode = self.mode as i32;
        self.settings.board_size = self.board_size;
        self.settings.handicap = self.handicap;
        self.settings.coordinates = self.coordinates;
        self.settings.sound = self.sound.enabled;
        self.settings.panel = self.panel_visible;
        self.settings.language = self.language;
        // On one line, and without names, which the settings file might not hold.
        self.settings.game = self.game.to_sgf("", "").replace('\n', " ");
        self.settings.window = self.placement;
        self.settings.maximized = self.maximized;
        self.settings.save();
    }

    // Input ------------------------------------------------------------------------------------------

    /// Where the ray through the pointer meets the plane at a height.
    fn pointer_on_plane(&self, height: f32) -> Vec3 {
        let (origin, direction) = self.camera.ray(self.pointer.x, self.pointer.y, self.width as f32, self.height as f32);
        if direction.y.abs() < 1e-5 {
            return origin;
        }
        let t = (height - origin.y) / direction.y;
        origin + direction * t.max(0.0)
    }

    /// The point nearest the pointer, aimed at a little above the board, between its surface and the tops of
    /// the stones.
    fn point_under_pointer(&self) -> Option<Point> {
        self.layout.point_near(self.pointer_on_plane(0.1))
    }

    /// Why a stone may not go on a point, for a toast.
    fn illegal_reason(&self, point: Point) -> Str {
        let board = self.game.current();
        if board.is_stone(point) {
            Str::Occupied
        } else if point == board.ko() {
            Str::KoForbidden
        } else if !board.is_legal(point, board.to_play()) {
            Str::Suicide
        } else {
            Str::Superko
        }
    }

    fn handle_board_pointer(&mut self) {
        let blocked = self.pointer.consumed || self.help_visible;
        let scale = self.scale;
        let playing = self.human_to_move() && self.view.board_ready();
        let counting = self.phase() == Phase::Scoring && self.mode != Mode::Demo && !self.finding_dead;
        let inside_window = self.pointer.x >= 0.0 && self.pointer.y >= 0.0 && self.pointer.x < self.width as f32 && self.pointer.y < self.height as f32;
        let point = if inside_window && !blocked { self.point_under_pointer() } else { None };
        // A click on the point would play a stone there, or mark the stones there dead or alive.
        let plays = |app: &App, point: Point| playing && app.game.is_legal(point);
        let marks = |app: &App, point: Point| counting && app.game.current().is_stone(point);

        // A legal point under the pointer, where a stone is shown seen through; while counting, a stone to mark.
        let hover_before = self.hover_point;
        self.hover_point = match self.gesture {
            Gesture::None | Gesture::PressPoint => point.filter(|point| plays(self, *point) || marks(self, *point)),
            Gesture::TurnCamera => None,
        };
        if self.hover_point.is_some() {
            self.cursor_hand = true;
        }
        if self.hover_point != hover_before {
            self.dirty = true; // drawn again with the new point and the stones it would take
        }

        if !blocked && self.pointer.left_pressed {
            (self.press_x, self.press_y) = (self.pointer.x, self.pointer.y);
            (self.drag_x, self.drag_y) = (self.pointer.x, self.pointer.y);
            self.press_point = point;
            self.gesture = if self.hover_point.is_some() { Gesture::PressPoint } else { Gesture::TurnCamera };
        }
        if !self.pointer.consumed && self.pointer.right_pressed && self.gesture == Gesture::None {
            self.gesture = Gesture::TurnCamera;
            self.press_point = None;
            (self.press_x, self.press_y) = (self.pointer.x, self.pointer.y);
            (self.drag_x, self.drag_y) = (self.pointer.x, self.pointer.y);
        }

        // A press on a point that moves on turns the view instead.
        let travel = (self.pointer.x - self.press_x).abs() + (self.pointer.y - self.press_y).abs();
        if self.gesture == Gesture::PressPoint && travel > 6.0 * scale && self.pointer.left_down {
            self.gesture = Gesture::TurnCamera;
        }
        if self.gesture == Gesture::TurnCamera {
            self.camera.rotate(-(self.pointer.x - self.drag_x) * 0.0055 / scale, (self.pointer.y - self.drag_y) * 0.0045 / scale);
            (self.drag_x, self.drag_y) = (self.pointer.x, self.pointer.y);
        }

        if self.pointer.left_released || self.pointer.right_released {
            let clicked = point.filter(|_| self.pointer.left_released);
            match (self.gesture, clicked) {
                (Gesture::PressPoint, Some(point)) if self.press_point == Some(point) => {
                    if plays(self, point) {
                        self.play_move(point);
                    } else if marks(self, point) {
                        self.game.toggle_dead(point);
                        self.sound.play(Effect::Click);
                    }
                }
                // A click where a stone may not go.
                (Gesture::TurnCamera, Some(point)) if travel < 4.0 * scale && playing && !self.game.is_legal(point) => {
                    self.show_toast(self.t(self.illegal_reason(point)));
                }
                _ => {}
            }
            let left_ends = self.pointer.left_released && self.gesture != Gesture::None && !(self.gesture == Gesture::TurnCamera && self.pointer.right_down);
            let right_ends = self.pointer.right_released && self.gesture == Gesture::TurnCamera && !self.pointer.left_down;
            if left_ends || right_ends {
                self.gesture = Gesture::None;
            }
        }

        if !self.pointer.consumed && self.pointer.wheel != 0.0 {
            self.camera.zoom(self.pointer.wheel);
        }
    }

    fn handle_key(&mut self, key: Key) {
        let control = self.modifiers.control_key();
        let shift = self.modifiers.shift_key();
        let step = if shift { -1 } else { 1 };

        if self.help_visible && matches!(key, Key::Escape | Key::F1) {
            self.help_visible = false;
            return;
        }

        if control {
            match key {
                Key::Character('Z') if shift => self.redo(),
                Key::Character('Z') => self.undo(),
                Key::Character('Y') => self.redo(),
                Key::Character('C') => self.copy_game(),
                Key::Character('V') => self.paste_game(),
                Key::Character('N') => self.new_game(),
                _ => {}
            }
            return;
        }

        match key {
            Key::Backspace | Key::Left => self.undo(),
            Key::Right => self.redo(),
            Key::Character('P') if self.human_to_move() => self.play_move(PASS),
            Key::Enter => self.accept(),
            Key::Character('N') => self.new_game(),
            Key::Character('H') => self.request_hint(),
            Key::Character('E') => self.request_estimate(),
            Key::Character('F') => self.camera.flip(),
            Key::Character('V') => self.camera.toggle_top_view(),
            Key::Home => self.camera.reset(self.view_side()),
            Key::Character('M') => self.set_mode(self.mode.stepped(step)),
            Key::Character('B') => self.set_board_size(self.stepped_board_size(step, true)),
            Key::Character('C') => self.toggle_coordinates(),
            Key::Character('T') => self.set_theme(self.theme as i32 + step),
            Key::Character('S') => self.toggle_sound(),
            Key::Character('L') => self.toggle_language(),
            Key::Tab => {
                self.panel_visible = !self.panel_visible;
                self.save();
            }
            Key::F1 => self.help_visible = !self.help_visible,
            Key::F11 => self.toggle_full_screen(),
            Key::Escape if self.full_screen => self.toggle_full_screen(),
            Key::Character(digit @ '1'..='5') => self.set_level(digit as i32 - '0' as i32),
            _ => {}
        }
    }

    // The interface ----------------------------------------------------------------------------------

    /// Whether a dialog is open over everything else.
    fn modal(&self) -> bool {
        self.help_visible
    }

    /// The rules note in lines that fit the help.
    fn rules_lines(&self) -> Vec<String> {
        let width = (HELP_WIDTH - 2.0 * HELP_PAD) * self.scale;
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in self.t(Str::RulesNote).split(' ') {
            let longer = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
            if !line.is_empty() && self.painter.measure(FontId::Body, &longer) > width {
                lines.push(std::mem::replace(&mut line, word.to_owned()));
            } else {
                line = longer;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }

    fn help_card(&self) -> Rect {
        let scale = self.scale;
        let rows = (HELP_ROWS + 1 + self.rules_lines().len()) as f32;
        let free = self.free_area();
        let (width, height) = ((HELP_WIDTH * scale).min(free.width - 20.0 * scale), (2.0 * HELP_PAD + 44.0 + 12.0 + rows * HELP_ROW) * scale);
        rect(free.x + (free.width - width) / 2.0, (10.0 * scale).max((self.height as f32 - height) / 2.0), width, height)
    }

    fn banner_card(&self) -> Rect {
        let scale = self.scale;
        let title = self.painter.measure(FontId::Display, &self.banner_title) + 80.0 * scale;
        let detail = self.painter.measure(FontId::Body, &self.banner_detail) + 60.0 * scale;
        let width = title.max(detail).max(320.0 * scale);
        let free = self.free_area();
        rect(free.x + (free.width - width) / 2.0, 26.0 * scale - (1.0 - self.banner_fade) * 20.0 * scale, width, 92.0 * scale)
    }

    fn frosted_panels(&self) -> Vec<FrostedPanel> {
        let mut panels = Vec::new();
        if self.panel_slide > 0.001 {
            panels.push(self.frosted(self.panel_rect(), 18.0, 1.0));
        }
        if self.banner_fade > 0.001 {
            panels.push(self.frosted(self.banner_card(), 18.0, self.banner_fade));
        }
        if self.help_visible {
            panels.push(self.frosted(self.help_card(), 20.0, 1.0));
        }
        panels
    }

    fn player_name(&self, side: Color) -> String {
        if self.mode == Mode::TwoPlayers {
            self.t(Str::Player).to_owned()
        } else if self.is_human(side) {
            self.t(Str::You).to_owned()
        } else {
            format!("{} · {}", self.t(Str::Computer), self.t(Str::level(self.level)))
        }
    }

    fn handicap_text(&self, stones: usize) -> String {
        match stones {
            0 | 1 => self.t(Str::NoHandicap).to_owned(),
            2..=4 => fill(self.t(Str::StonesFew), &[&stones]),
            _ => fill(self.t(Str::StonesMany), &[&stones]),
        }
    }

    /// The winner, once the game is over.
    fn result_title(&self) -> &'static str {
        self.t(match self.game.winner() {
            Some(Color::Black) => Str::BlackWins,
            Some(Color::White) => Str::WhiteWins,
            None => Str::Draw,
        })
    }

    /// How the game was won: who resigned, or the points of each side.
    fn result_detail(&self) -> String {
        match self.game.moves().last() {
            None => String::new(),
            Some(last) if last.is_resignation() => self.t(if last.color == Color::Black { Str::BlackResigned } else { Str::WhiteResigned }).to_owned(),
            Some(_) => {
                let score = self.game.count();
                format!("{}  ·  {}", fill(self.t(Str::ScoreDetail), &[&score.black, &score.white, &score.komi]), self.game.result_text())
            }
        }
    }

    fn draw_interface(&mut self) {
        self.draw_panel();
        self.draw_banner();
        self.draw_toasts();
        if self.help_visible {
            self.draw_help();
        }
    }

    fn draw_status(&mut self, area: Rect) {
        let scale = self.scale;
        let phase = self.phase();
        let side = self.game.current().to_play();
        let accent = self.accent();
        let thinking = self.computer_thinking || self.computer_result.is_some();
        self.painter.fill(area, 12.0 * scale, rgba(255, 255, 255, 14));

        // A stone of the side to move, or of the winner.
        let winner = if phase == Phase::Finished { self.game.winner() } else { None };
        draw_stone(&mut self.painter, area.x + 22.0 * scale, area.y + area.height / 2.0, 10.0 * scale, winner.unwrap_or(side));

        let (title, detail, title_color) = match phase {
            Phase::Finished => (self.result_title(), if self.game.resigned() { self.result_detail() } else { self.game.result_text() }, accent),
            Phase::Scoring => (self.t(Str::Counting), self.t(if self.finding_dead { Str::Estimating } else { Str::CountingHelp }).to_owned(), TEXT_COLOR),
            Phase::Playing => {
                let title = self.t(if side == Color::Black { Str::BlackToMove } else { Str::WhiteToMove });
                (title, if thinking { self.t(Str::Thinking).to_owned() } else { self.player_name(side) }, TEXT_COLOR)
            }
        };
        let text_x = area.x + 44.0 * scale;
        let line_height = self.painter.line_height(FontId::Heading);
        self.painter.text(FontId::Heading, text_x, area.y + area.height / 2.0 - line_height + 3.0 * scale, title, title_color);
        self.painter.text(FontId::Body, text_x, area.y + area.height / 2.0 + 2.0 * scale, &detail, MUTED_COLOR);
        if thinking || self.hint_thinking || self.estimating || self.finding_dead {
            self.painter.spinner(area.right() - 22.0 * scale, area.y + area.height / 2.0, 8.0 * scale, 2.2 * scale, self.time * 5.0, accent);
        }
    }

    /// The players with their prisoners while the game goes on; their points when it is counted.
    fn draw_score(&mut self, area: Rect) {
        let scale = self.scale;
        let phase = self.phase();
        let counted = self.counted();
        let score = self.game.count();
        let row_height = 28.0 * scale;
        let body_height = self.painter.line_height(FontId::Body);
        // What the numbers are, over them.
        let caption = self.t(if counted { Str::Points } else { Str::Captures });
        self.painter.text(FontId::Body, area.right() - self.painter.measure(FontId::Body, caption), area.y, caption, MUTED_COLOR.fade(0.8));
        let mut y = area.y + body_height + 2.0 * scale;
        for side in [Color::Black, Color::White] {
            let to_move = phase == Phase::Playing && self.game.current().to_play() == side;
            draw_stone(&mut self.painter, area.x + 9.0 * scale, y + row_height / 2.0, 7.0 * scale, side);
            let who = if self.mode == Mode::TwoPlayers {
                Str::Player
            } else if self.is_human(side) {
                Str::You
            } else {
                Str::Computer
            };
            let name = format!("{}  ·  {}", self.t(if side == Color::Black { Str::Black } else { Str::White }), self.t(who));
            self.painter.text(
                FontId::Body,
                area.x + 26.0 * scale,
                y + (row_height - body_height) / 2.0,
                &name,
                if to_move { TEXT_COLOR } else { TEXT_COLOR.fade(0.75) },
            );
            let value = match (counted, side) {
                (true, Color::Black) => score.black.to_string(),
                (true, Color::White) => format!("{} + {}", score.white, score.komi),
                (false, _) => self.game.current().captures(side).to_string(),
            };
            let value_width = self.painter.measure(FontId::Heading, &value);
            let value_y = y + (row_height - self.painter.line_height(FontId::Heading)) / 2.0;
            self.painter.text(FontId::Heading, area.right() - value_width, value_y, &value, TEXT_COLOR);
            y += row_height;
        }
        y += 8.0 * scale;
        if counted {
            // A bar shared out between the colors, komi with White.
            let bar = rect(area.x, y, area.width, 6.0 * scale);
            let total = (f64::from(score.black + score.white) + score.komi).max(1.0);
            let share = (f64::from(score.black) / total) as f32;
            self.painter.fill(bar, 3.0 * scale, rgba(238, 236, 230, 220));
            self.painter.fill(rect(bar.x, bar.y, bar.height.max(bar.width * share), bar.height), 3.0 * scale, rgba(10, 10, 12, 235));
            self.painter.fill(rect(bar.x + bar.width / 2.0 - 0.5 * scale, bar.y - 2.0 * scale, scale, bar.height + 4.0 * scale), 0.0, rgba(255, 255, 255, 90));
        } else {
            let size = self.game.size();
            let mut line = format!("{size} × {size}  ·  {} {}", self.t(Str::Komi), self.game.komi());
            if self.game.handicap() >= 2 {
                line += &format!("  ·  {}: {}", self.t(Str::Handicap), self.game.handicap());
            }
            self.painter.text(FontId::Body, area.x, y - 4.0 * scale, &line, MUTED_COLOR);
        }
    }

    fn draw_move_list(&mut self, area: Rect) {
        let scale = self.scale;
        self.painter.fill(area, 12.0 * scale, rgba(0, 0, 0, 40));
        let count = self.game.moves().len();
        if count == 0 {
            self.painter.text_centered(FontId::Body, area, self.t(Str::NoMovesYet), MUTED_COLOR);
            return;
        }

        let row_height = (25.0 * scale).round();
        let inner = area.inset(8.0 * scale);
        let visible = ((inner.height / row_height) as usize).max(1);
        // Each row starts with Black's move; with a handicap White begins, in the second column.
        let offset = usize::from(self.game.start().to_play() == Color::White);
        let rows = (count + offset).div_ceil(2);
        let max_scroll = rows.saturating_sub(visible) as f32;

        if area.contains(self.pointer.x, self.pointer.y) && self.pointer.wheel != 0.0 && !self.help_visible {
            self.move_scroll = (self.move_scroll - self.pointer.wheel * 3.0).clamp(0.0, max_scroll);
            self.follow_moves = self.move_scroll >= max_scroll;
            self.pointer.wheel = 0.0;
        }
        if self.follow_moves {
            self.move_scroll = max_scroll;
        }
        self.move_scroll = self.move_scroll.clamp(0.0, max_scroll);

        let first = self.move_scroll as usize;
        let number_width = 40.0 * scale;
        let column_width = (inner.width - number_width) / 2.0;
        let accent = self.accent();
        for row in first..rows.min(first + visible) {
            let y = inner.y + (row - first) as f32 * row_height;
            let number = format!("{}.", row + 1);
            let number_x = inner.x + number_width - 10.0 * scale - self.painter.measure(FontId::Body, &number);
            self.painter.text(FontId::Body, number_x, y + 3.0 * scale, &number, MUTED_COLOR);
            for column in 0..2 {
                let Some(ply) = (row * 2 + column).checked_sub(offset).filter(|ply| *ply < count) else { continue };
                let x = inner.x + number_width + column as f32 * column_width;
                if ply == count - 1 {
                    self.painter.fill(rect(x - 6.0 * scale, y + scale, column_width - 6.0 * scale, row_height - 2.0 * scale), 7.0 * scale, accent.fade(0.28));
                }
                let record = &self.game.moves()[ply];
                if record.is_pass() || record.is_resignation() {
                    let word = self.t(if record.is_pass() { Str::Pass } else { Str::Resign });
                    self.painter.text(FontId::Body, x, y + 3.0 * scale, word, MUTED_COLOR);
                } else {
                    // The point, and the stones it took.
                    let (name, taken) = (go::point_name(record.point, self.game.size()), record.captured.len());
                    let width = self.painter.text(FontId::Body, x, y + 3.0 * scale, &name, TEXT_COLOR);
                    if taken > 0 {
                        self.painter.text(FontId::Body, x + width + 8.0 * scale, y + 3.0 * scale, &format!("×{taken}"), MUTED_COLOR.fade(0.85));
                    }
                }
            }
        }
        if max_scroll > 0.0 {
            // A thin scroll bar.
            let bar_height = inner.height * visible as f32 / rows as f32;
            let bar_y = inner.y + (inner.height - bar_height) * (self.move_scroll / max_scroll);
            self.painter.fill(rect(area.right() - 5.0 * scale, bar_y, 3.0 * scale, bar_height), 1.5 * scale, rgba(255, 255, 255, 50));
        }
    }

    fn draw_panel(&mut self) {
        if self.panel_slide <= 0.001 {
            return;
        }
        let scale = self.scale;
        let panel = self.panel_rect();
        if panel.contains(self.pointer.x, self.pointer.y) && (self.pointer.left_pressed || self.pointer.right_pressed) {
            self.pointer.consumed = true;
        }
        self.painter.shadow(panel, 22.0 * scale, rgba(0, 0, 0, 70));
        self.painter.fill(panel, 18.0 * scale, rgba(16, 18, 26, 120));
        self.painter.outline(panel, 18.0 * scale, rgba(255, 255, 255, 24));

        // In a low window everything is packed closer, and the footer goes.
        let compact = panel.height < 760.0 * scale;
        let sized = |roomy: f32, packed: f32| (if compact { packed } else { roomy }) * scale;
        let pad = sized(20.0, 16.0);
        let (x, width) = (panel.x + pad, panel.width - 2.0 * pad);
        let mut y = panel.y + pad;

        self.painter.text(FontId::Heading, x, y, self.t(Str::Title), TEXT_COLOR);
        if self.button(HELP_BUTTON, rect(panel.right() - pad - 30.0 * scale, y - 2.0 * scale, 30.0 * scale, 30.0 * scale), "?", true, false) {
            self.help_visible = !self.help_visible;
        }
        y += sized(44.0, 38.0);

        let status_height = sized(62.0, 54.0);
        self.draw_status(rect(x, y, width, status_height));
        y += status_height + sized(14.0, 8.0);
        let score_height = 2.0 * self.painter.line_height(FontId::Body) + 66.0 * scale;
        self.draw_score(rect(x, y, width, score_height));
        y += score_height + sized(10.0, 4.0);

        // From the bottom up: the footer, the switches, the settings and the buttons; the moves take the space between.
        let mut bottom = panel.bottom() - pad;
        if !compact {
            let footer_height = self.painter.line_height(FontId::Body);
            self.painter.text_centered(FontId::Body, rect(x, bottom - footer_height, width, footer_height), self.t(Str::Footer), MUTED_COLOR.fade(0.8));
            bottom -= footer_height + 14.0 * scale;
        }

        let (row_height, gap) = (sized(34.0, 29.0), sized(8.0, 5.0));
        let (half, third) = ((width - gap) / 2.0, (width - 2.0 * gap) / 3.0);
        bottom -= row_height;
        if self.button(SOUND_BUTTON, rect(x, bottom, third, row_height), self.t(Str::Sound), true, self.sound.enabled) {
            self.toggle_sound();
        }
        if self.button(COORDINATES_BUTTON, rect(x + third + gap, bottom, third, row_height), self.t(Str::Coordinates), true, self.coordinates) {
            self.toggle_coordinates();
        }
        if self.button(LANGUAGE_BUTTON, rect(x + 2.0 * (third + gap), bottom, third, row_height), self.t(Str::LanguageName), true, false) {
            self.toggle_language();
        }
        bottom -= gap;

        let theme = self.theme();
        let mode_name = [Str::ModeBlack, Str::ModeWhite, Str::ModeTwoPlayers, Str::ModeDemo][self.mode as usize];
        let choices = [
            (THEME_SELECTOR, Str::Theme, (if self.language == Language::Russian { theme.name_russian } else { theme.name_english }).to_owned()),
            (HANDICAP_SELECTOR, Str::Handicap, self.handicap_text(self.handicap)),
            (BOARD_SELECTOR, Str::Board, format!("{0} × {0}", self.board_size)),
            (LEVEL_SELECTOR, Str::Level, format!("{} · {}", self.level, self.t(Str::level(self.level)))),
            (MODE_SELECTOR, Str::Mode, self.t(mode_name).to_owned()),
        ];
        for (id, label, value) in choices {
            bottom -= row_height;
            let change = self.selector(id, rect(x, bottom, width, row_height), self.t(label), &value);
            if change != 0 {
                match id {
                    THEME_SELECTOR => self.set_theme(self.theme as i32 + change),
                    HANDICAP_SELECTOR => self.step_handicap(change),
                    BOARD_SELECTOR => self.set_board_size(self.stepped_board_size(change, false)),
                    LEVEL_SELECTOR => self.set_level(self.level + change),
                    _ => self.set_mode(self.mode.stepped(change)),
                }
            }
            bottom -= sized(6.0, 3.0);
        }
        bottom -= sized(8.0, 4.0);

        let button_height = sized(38.0, 31.0);
        bottom -= button_height;
        if self.button(NEW_GAME_BUTTON, rect(x, bottom, half, button_height), self.t(Str::NewGame), true, false) {
            self.new_game();
        }
        if self.button(FLIP_BUTTON, rect(x + half + gap, bottom, half, button_height), self.t(Str::Flip), true, false) {
            self.camera.flip();
        }
        bottom -= gap + button_height;
        if self.button(UNDO_BUTTON, rect(x, bottom, third, button_height), self.t(Str::Undo), self.game.can_undo(), false) {
            self.undo();
        }
        if self.button(REDO_BUTTON, rect(x + third + gap, bottom, third, button_height), self.t(Str::Redo), self.game.can_redo(), false) {
            self.redo();
        }
        let can_hint = self.human_to_move() && !self.hint_thinking;
        if self.button(HINT_BUTTON, rect(x + 2.0 * (third + gap), bottom, third, button_height), self.t(Str::Hint), can_hint, true) {
            self.request_hint();
        }
        bottom -= gap + button_height;
        let phase = self.phase();
        let watching = self.mode == Mode::Demo;
        if phase == Phase::Scoring {
            // The count: accepted as marked, or the game played on.
            if self.button(ACCEPT_BUTTON, rect(x, bottom, half, button_height), self.t(Str::Accept), !watching && !self.finding_dead, true) {
                self.accept();
            }
            if self.button(RESUME_BUTTON, rect(x + half + gap, bottom, half, button_height), self.t(Str::Resume), !watching, false) {
                self.undo();
            }
        } else {
            if self.button(PASS_BUTTON, rect(x, bottom, third, button_height), self.t(Str::Pass), self.human_to_move(), false) {
                self.play_move(PASS);
            }
            // Resigning takes a second click.
            let confirming = self.resign_confirm > 0.0;
            let resign = self.t(if confirming { Str::ConfirmResign } else { Str::Resign });
            if self.button(RESIGN_BUTTON, rect(x + third + gap, bottom, third, button_height), resign, phase == Phase::Playing && !watching, confirming) {
                if confirming {
                    self.resign(if self.mode == Mode::TwoPlayers { self.game.current().to_play() } else { self.view_side() });
                } else {
                    self.resign_confirm = 3.0;
                }
            }
            let can_estimate = phase == Phase::Playing && !self.estimating;
            if self.button(
                ESTIMATE_BUTTON,
                rect(x + 2.0 * (third + gap), bottom, third, button_height),
                self.t(Str::Estimate),
                can_estimate,
                self.estimate.is_some(),
            ) {
                self.request_estimate();
            }
        }
        bottom -= sized(16.0, 10.0);

        // The moves, where there is room for them.
        let label_height = self.painter.line_height(FontId::Body) + 6.0 * scale;
        if bottom - y - label_height > 40.0 * scale {
            self.painter.text(FontId::Body, x, y, self.t(Str::Moves), MUTED_COLOR);
            self.draw_move_list(rect(x, y + label_height, width, bottom - y - label_height));
        }
    }

    fn draw_banner(&mut self) {
        if self.banner_fade <= 0.001 {
            return;
        }
        let scale = self.scale;
        let fade = self.banner_fade;
        let card = self.banner_card();
        self.painter.shadow(card, 24.0 * scale, rgba(0, 0, 0, 120).fade(fade));
        self.painter.fill(card, 18.0 * scale, rgba(14, 16, 24, 150).fade(fade));
        self.painter.outline(card, 18.0 * scale, self.accent().fade(0.6 * fade));
        self.painter.text_centered(FontId::Display, rect(card.x, card.y + 8.0 * scale, card.width, 48.0 * scale), &self.banner_title, TEXT_COLOR.fade(fade));
        self.painter.text_centered(FontId::Body, rect(card.x, card.y + 54.0 * scale, card.width, 26.0 * scale), &self.banner_detail, MUTED_COLOR.fade(fade));
    }

    fn draw_help(&mut self) {
        let scale = self.scale;
        self.painter.fill(rect(0.0, 0.0, self.width as f32, self.height as f32), 0.0, rgba(0, 0, 0, 110));

        let (row_height, pad) = (HELP_ROW * scale, HELP_PAD * scale);
        let card = self.help_card();
        let accent = self.accent();
        self.painter.shadow(card, 26.0 * scale, rgba(0, 0, 0, 140));
        self.painter.fill(card, 20.0 * scale, rgba(18, 20, 28, 175));
        self.painter.outline(card, 20.0 * scale, rgba(255, 255, 255, 30));
        self.painter.text(FontId::Heading, card.x + pad, card.y + pad - 4.0 * scale, self.t(Str::Controls), TEXT_COLOR);
        let top = card.y + pad + 44.0 * scale;
        // The first column takes a little over half the card, or what its longest line needs.
        let (left_keys, right_keys) = (165.0 * scale, 125.0 * scale);
        let longest = HELP_LINES[..HELP_ROWS].iter().map(|(_, _, action)| self.painter.measure(FontId::Body, self.t(*action))).fold(0.0, f32::max);
        let left_width = ((card.width - 2.0 * pad) * 0.56).max(left_keys + longest + 20.0 * scale);
        for (i, (named, keys, action)) in HELP_LINES.into_iter().enumerate() {
            let (column_x, keys_width) = if i < HELP_ROWS { (card.x + pad, left_keys) } else { (card.x + pad + left_width, right_keys) };
            let y = top + (i % HELP_ROWS) as f32 * row_height;
            self.painter.text(FontId::Body, column_x, y, named.map_or(keys, |id| self.t(id)), accent);
            self.painter.text(FontId::Body, column_x + keys_width, y, self.t(action), TEXT_COLOR);
        }
        // The rules, in a few sentences.
        let mut y = top + HELP_ROWS as f32 * row_height + 12.0 * scale;
        self.painter.text(FontId::Body, card.x + pad, y, self.t(Str::Rules), accent);
        for line in self.rules_lines() {
            y += row_height;
            self.painter.text(FontId::Body, card.x + pad, y, &line, MUTED_COLOR);
        }

        let close = self.button(HELP_CLOSE, rect(card.right() - pad - 30.0 * scale, card.y + pad - 8.0 * scale, 30.0 * scale, 30.0 * scale), "×", true, false);
        if close || (self.pointer.left_pressed && !card.contains(self.pointer.x, self.pointer.y)) {
            self.help_visible = false;
        }
        if self.pointer.left_pressed || self.pointer.right_pressed {
            self.pointer.consumed = true;
        }
    }
}
