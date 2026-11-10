//! The computer player. Monte Carlo tree search: playouts with the usual heuristics of Go programs (answering
//! ataris, taking stones, 3×3 shapes near the last move, no filling of eyes or putting big strings in atari), a
//! tree with priors and all-moves-as-first (RAVE) statistics, and independent trees on several threads whose
//! results are added up.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::go::{Board, Cell, Color, PASS, POINT_COUNT, Point, PointSet, STRIDE, area_owners, neighbours, point_at, x_of, y_of};

#[derive(Clone, Copy)]
pub struct SearchLimits {
    pub playouts: usize, // in all, over the threads
    pub milliseconds: u64,
    pub threads: usize,
    pub may_resign: bool,
    pub seed: u64,
}

#[derive(Clone, Copy)]
pub struct SearchResult {
    pub point: Point, // where to play, or `PASS`
    pub resign: bool,
    pub win_rate: f64, // for the side to move
    pub playouts: u64,
}

/// Who owns each point at the end of the game, as the share of playouts in which Black did, and the mean score.
pub struct Ownership {
    pub black: [f32; POINT_COUNT],
    pub margin: f64, // Black's points less White's and komi
}

impl Ownership {
    /// The stones that are probably dead, for scoring.
    pub fn dead(&self, board: &Board) -> PointSet {
        std::array::from_fn(|point| match board.at(point) {
            Cell::Black => self.black[point] < 0.5,
            Cell::White => self.black[point] > 0.5,
            _ => false,
        })
    }
}

/// A fast generator for the playouts (xorshift64*).
struct Random(u64);

impl Random {
    fn new(seed: u64) -> Random {
        Random(if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed })
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// From 0 to n - 1.
    fn below(&mut self, n: usize) -> usize {
        (((self.next() >> 32) * n as u64) >> 32) as usize
    }
}

// 3×3 shapes ----------------------------------------------------------------------------------------

/// The eight points round a point, in the order of their bits in a shape's code.
const fn around(point: Point) -> [Point; 8] {
    [point - STRIDE - 1, point - STRIDE, point - STRIDE + 1, point - 1, point + 1, point + STRIDE - 1, point + STRIDE, point + STRIDE + 1]
}

/// Local shapes in which the middle point is a good move (from MoGo and Pachi): X and O are the two colors either
/// way round, unless a tenth character says X is to play; x is anything but X, o anything but O, # off the
/// board, ? anything.
const SHAPES: [&str; 15] = [
    "XOX...???",  // hane, enclosing
    "XO....?.?",  // hane, not cutting
    "XO?X..x.?",  // hane, bending round
    "XOO...?.?X", // thin hane
    ".O.X.....",  // diagonal attachment
    "XO?O.o?o?",  // cut, unprotected
    "XO?O.X???",  // cut, peeped
    "?X?O.Oooo",  // pushing through
    "OX?o.O???",  // cutting a knight's move
    "X.?O.?##?",  // edge: chasing
    "OX?X.O###",  // edge: blocking a cut
    "?X?x.O###",  // edge: blocking a connection
    "?XOx.x###",  // edge: dropping back
    "?OXo.O?##X", // edge: throwing in
    "?OXX.O###X", // edge: cutting
];

const BLACK_TO_PLAY: u8 = 1;
const WHITE_TO_PLAY: u8 = 2;

/// For each arrangement of the eight points round an empty point (two bits each, as `Cell`), whether a shape
/// matches with Black to play and with White to play.
static SHAPE_TABLE: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let mut table = vec![0u8; 1 << 16];
    for shape in SHAPES.map(str::as_bytes) {
        let x_to_play = shape.get(9) == Some(&b'X');
        for symmetry in 0..8 {
            // The shape's character at each of the eight points, turned and mirrored.
            let mut wanted = [b'?'; 8];
            for (index, character) in shape[..9].iter().enumerate() {
                let (mut dx, mut dy) = (index as i32 % 3 - 1, index as i32 / 3 - 1);
                if (dx, dy) == (0, 0) {
                    continue;
                }
                if symmetry & 1 != 0 {
                    dx = -dx;
                }
                if symmetry & 2 != 0 {
                    dy = -dy;
                }
                if symmetry & 4 != 0 {
                    (dx, dy) = (dy, dx);
                }
                // The slots go row by row, the middle left out.
                let place = ((dy + 1) * 3 + dx + 1) as usize;
                wanted[if place < 4 { place } else { place - 1 }] = *character;
            }
            for (x, o, to_play) in [(Cell::Black, Cell::White, BLACK_TO_PLAY), (Cell::White, Cell::Black, WHITE_TO_PLAY)] {
                let players = if x_to_play { to_play } else { BLACK_TO_PLAY | WHITE_TO_PLAY };
                // The cells each point may hold, as bits.
                let allowed = wanted.map(|character| {
                    let allows = |cell: Cell| match character {
                        b'X' => cell == x,
                        b'O' => cell == o,
                        b'.' => cell == Cell::Empty,
                        b'#' => cell == Cell::Border,
                        b'x' => cell != x,
                        b'o' => cell != o,
                        _ => true,
                    };
                    Cell::ALL.into_iter().filter(|cell| allows(*cell)).fold(0u8, |bits, cell| bits | 1 << cell as u8)
                });
                for (code, entry) in table.iter_mut().enumerate() {
                    if (0..8).all(|slot| allowed[slot] >> ((code >> (2 * slot)) & 3) & 1 != 0) {
                        *entry |= players;
                    }
                }
            }
        }
    }
    table
});

fn matches_shape(table: &[u8], board: &Board, point: Point, to_play: Color) -> bool {
    let code = around(point).into_iter().rev().fold(0, |code, next| code << 2 | board.at(next) as usize);
    let player = if to_play == Color::Black { BLACK_TO_PLAY } else { WHITE_TO_PLAY };
    table[code] & player != 0
}

// Playouts --------------------------------------------------------------------------------------------

/// Where each point was first played in a simulation, and by whom, for the all-moves-as-first statistics.
struct FirstMoves {
    ply: [i16; POINT_COUNT],
    color: [Color; POINT_COUNT],
}

impl FirstMoves {
    fn new() -> FirstMoves {
        FirstMoves { ply: [-1; POINT_COUNT], color: [Color::Black; POINT_COUNT] }
    }

    fn clear(&mut self) {
        self.ply.fill(-1);
    }

    fn record(&mut self, point: Point, by: Color, at: usize) {
        if point != PASS && self.ply[point] < 0 {
            self.ply[point] = at as i16;
            self.color[point] = by;
        }
    }
}

struct Playout {
    random: Random,
    shapes: &'static [u8],
    moves: [Point; 48], // the candidates of a heuristic
    count: usize,
}

impl Playout {
    fn new(seed: u64) -> Playout {
        Playout { random: Random::new(seed), shapes: SHAPE_TABLE.as_slice(), moves: [PASS; 48], count: 0 }
    }

    /// A move for the side to play: answers to the last move first, then any move that does not fill an eye of
    /// one's own or put a string in atari for nothing.
    fn choose(&mut self, board: &Board) -> Point {
        let me = board.to_play();
        let last = board.last_move();
        if last != PASS && board.is_stone(last) {
            if self.random.below(10) < 9 && self.tactics(board, last, me) {
                return self.pick();
            }
            if self.random.below(10) < 9 && self.shapes(board, last, me) {
                return self.pick();
            }
        }
        let empty = board.empty_points();
        if empty.is_empty() {
            return PASS;
        }
        let start = self.random.below(empty.len());
        for offset in 0..empty.len() {
            let point = Point::from(empty[(start + offset) % empty.len()]);
            if !board.is_legal(point, me) || board.is_eye(point, me) {
                continue;
            }
            // Putting a string in atari for nothing is left out; a single stone now and then, as a throw-in.
            if board.is_self_atari(point, me) && (joins_string(board, point, me) || self.random.below(10) < 8) {
                continue;
            }
            return point;
        }
        PASS
    }

    /// Plays the position out to the end, both sides passing; Black's score less komi.
    fn run(&mut self, board: &mut Board, komi: f64, mut first: Option<&mut FirstMoves>, mut ply: usize) -> f64 {
        let limit = board.size() * board.size() * 3;
        let mut passes = 0;
        for _ in 0..limit {
            if passes >= 2 {
                break;
            }
            let me = board.to_play();
            let point = self.choose(board);
            if let Some(first) = first.as_deref_mut() {
                first.record(point, me, ply);
            }
            board.play(point, me, None);
            passes = if point == PASS { passes + 1 } else { 0 };
            ply += 1;
        }
        f64::from(board.quick_score()) - komi
    }

    fn add(&mut self, board: &Board, point: Point, me: Color) {
        if self.count < self.moves.len() && board.is_legal(point, me) {
            self.moves[self.count] = point;
            self.count += 1;
        }
    }

    fn pick(&mut self) -> Point {
        self.moves[self.random.below(self.count)]
    }

    /// Taking a string in atari next to the last move, or saving one's own.
    fn tactics(&mut self, board: &Board, last: Point, me: Color) -> bool {
        self.count = 0;
        let own = me.cell();
        if board.in_atari(last) {
            self.add(board, board.atari_liberty(last), me);
        }
        for next in neighbours(last) {
            if !board.is_stone(next) || !board.in_atari(next) {
                continue;
            }
            if board.at(next) != own {
                self.add(board, board.atari_liberty(next), me);
                continue;
            }
            // Our string in atari: run, if that gives it liberties, or take a string of theirs touching it.
            let liberty = board.atari_liberty(next);
            if !board.is_self_atari(liberty, me) {
                self.add(board, liberty, me);
            }
            let head = board.head(next);
            let mut stone = head;
            for _ in 0..12 {
                for enemy in neighbours(stone) {
                    if board.is_stone(enemy) && board.at(enemy) != own && board.in_atari(enemy) {
                        self.add(board, board.atari_liberty(enemy), me);
                    }
                }
                stone = board.next_stone(stone);
                if stone == head {
                    break;
                }
            }
        }
        self.count > 0
    }

    /// The shapes round the last move.
    fn shapes(&mut self, board: &Board, last: Point, me: Color) -> bool {
        self.count = 0;
        for point in around(last) {
            if board.at(point) == Cell::Empty && matches_shape(self.shapes, board, point, me) && !board.is_self_atari(point, me) {
                self.add(board, point, me);
            }
        }
        self.count > 0
    }
}

fn joins_string(board: &Board, point: Point, me: Color) -> bool {
    neighbours(point).into_iter().any(|next| board.at(next) == me.cell())
}

/// Adds up who owns each point at the end of a playout.
fn count_owners(board: &Board, black: &mut [u32; POINT_COUNT], white: &mut [u32; POINT_COUNT]) {
    for point in board.points() {
        match board.settled_owner(point) {
            Cell::Black => black[point] += 1,
            Cell::White => white[point] += 1,
            _ => {}
        }
    }
}

// The tree --------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct Node {
    point: u16,       // the move, or `PASS`
    child_count: u16, // none until the node is expanded
    first_child: u32,
    // For the side that played the move, the prior knowledge included.
    visits: f32,
    wins: f32,
    rave_visits: f32,
    rave_wins: f32,
    simulations: u32, // through the node itself
}

impl Node {
    fn expanded(&self) -> bool {
        self.child_count > 0
    }

    fn children(&self) -> std::ops::Range<usize> {
        let first = self.first_child as usize;
        first..first + usize::from(self.child_count)
    }
}

const EQUIVALENCE: f32 = 3000.0; // how long the all-moves-as-first statistics weigh in
const EXPAND_AFTER: u32 = 8; // simulations
const MAX_NODES: usize = 2_000_000;

/// Prior knowledge of a move, as the visits and wins it starts with.
struct Prior {
    visits: f32,
    wins: f32,
}

impl Prior {
    fn good(&mut self, weight: f32) {
        self.visits += weight / 2.0;
        self.wins += weight / 2.0;
    }

    fn bad(&mut self, weight: f32) {
        self.visits += weight / 2.0;
    }
}

/// A node for a move with what is known of it beforehand: captures, escapes and good shapes are favored, as are
/// lines three and four early on; self-ataris and the first line early on are not.
fn node_with_prior(board: &Board, point: Point, me: Color, shapes: &[u8]) -> Node {
    let mut prior = Prior { visits: 10.0, wins: 5.0 };
    if point != PASS {
        weigh(&mut prior, board, point, me, shapes);
    }
    Node { point: point as u16, visits: prior.visits, wins: prior.wins, ..Node::default() }
}

fn weigh(prior: &mut Prior, board: &Board, point: Point, me: Color, shapes: &[u8]) {
    let own = me.cell();
    let self_atari = board.is_self_atari(point, me);
    for next in neighbours(point) {
        if board.is_stone(next) && board.in_atari(next) {
            if board.at(next) != own {
                prior.good(if board.stone_count(next) > 1 { 30.0 } else { 15.0 });
            } else if !self_atari {
                prior.good(15.0);
            }
        }
    }
    if self_atari {
        prior.bad(if joins_string(board, point, me) { 30.0 } else { 10.0 }); // a lone stone may be a throw-in
    }
    let (x, y) = (x_of(point), y_of(point));
    let last = board.last_move();
    if last != PASS && board.is_stone(last) {
        let (dx, dy) = (x.abs_diff(x_of(last)), y.abs_diff(y_of(last)));
        if dx <= 1 && dy <= 1 && matches_shape(shapes, board, point, me) {
            prior.good(10.0);
        } else if dx + dy <= 3 {
            prior.good(4.0);
        }
    }
    // Early on, the third and fourth lines, the corners first; not the edges.
    let size = board.size();
    let (from_side, from_top) = (x.min(size - 1 - x), y.min(size - 1 - y));
    let line = 1 + from_side.min(from_top);
    if board.empty_points().len() > size * size * 3 / 4 {
        match line {
            1 => prior.bad(30.0),
            2 => prior.bad(20.0),
            3 => prior.good(12.0),
            4 if size >= 13 => prior.good(12.0),
            _ => {}
        }
        // A corner point while the corner is empty.
        if size >= 13 && (2..=3).contains(&from_side) && (2..=3).contains(&from_top) {
            let (corner_x, corner_y) = (if x < size / 2 { 0 } else { size - 6 }, if y < size / 2 { 0 } else { size - 6 });
            if is_empty_square(board, corner_x, corner_y, 6) {
                prior.good(20.0);
            }
        }
    } else if line == 1 {
        prior.bad(5.0);
    }
}

/// Whether there is no stone on a square of the board, given by its corner and its side.
fn is_empty_square(board: &Board, left: usize, top: usize, side: usize) -> bool {
    (top..top + side).all(|y| (left..left + side).all(|x| board.at(point_at(x, y)) == Cell::Empty))
}

/// What a tree found: the visits and wins of the moves at the root, and who owned each point in its playouts.
struct RootStats {
    visits: Vec<f64>,
    wins: Vec<f64>,
    black: [u32; POINT_COUNT],
    white: [u32; POINT_COUNT],
    playouts: u64,
}

impl RootStats {
    fn new(moves: usize) -> RootStats {
        RootStats { visits: vec![0.0; moves], wins: vec![0.0; moves], black: [0; POINT_COUNT], white: [0; POINT_COUNT], playouts: 0 }
    }

    fn add(&mut self, other: &RootStats) {
        let sum = |total: &mut [f64], part: &[f64]| total.iter_mut().zip(part).for_each(|(total, part)| *total += part);
        sum(&mut self.visits, &other.visits);
        sum(&mut self.wins, &other.wins);
        for point in 0..POINT_COUNT {
            self.black[point] += other.black[point];
            self.white[point] += other.white[point];
        }
        self.playouts += other.playouts;
    }
}

struct Tree<'a> {
    root: &'a Board,
    komi: f64,
    playout: Playout,
    nodes: Vec<Node>,
    path: Vec<usize>,
    first: FirstMoves,
}

impl Tree<'_> {
    fn new<'a>(root: &'a Board, moves: &[Point], komi: f64, seed: u64) -> Tree<'a> {
        let playout = Playout::new(seed);
        let mut nodes = Vec::with_capacity(MAX_NODES.min(4096 + moves.len()));
        nodes.push(Node { first_child: 1, child_count: moves.len() as u16, ..Node::default() });
        nodes.extend(moves.iter().map(|point| node_with_prior(root, *point, root.to_play(), playout.shapes)));
        Tree { root, komi, playout, nodes, path: Vec::new(), first: FirstMoves::new() }
    }

    fn simulate(&mut self, stats: &mut RootStats) {
        let mut board = self.root.clone();
        let root_color = self.root.to_play();
        self.first.clear();
        self.path.clear();
        self.path.push(0);
        let (mut node, mut ply) = (0, 0);
        while self.nodes[node].expanded() && board.passes() < 2 {
            node = self.select(node);
            let point = Point::from(self.nodes[node].point);
            let mover = board.to_play();
            self.first.record(point, mover, ply);
            ply += 1;
            board.play(point, mover, None);
            self.path.push(node);
            // A leaf that has been visited enough grows its children.
            let leaf = self.nodes[node];
            if !leaf.expanded() && leaf.simulations >= EXPAND_AFTER && board.passes() < 2 {
                self.expand(node, &board);
            }
        }
        let score =
            if board.passes() >= 2 { f64::from(board.quick_score()) - self.komi } else { self.playout.run(&mut board, self.komi, Some(&mut self.first), ply) };
        let black_wins = score > 0.0;
        count_owners(&board, &mut stats.black, &mut stats.white);
        stats.playouts += 1;

        // Back up the result, and the all-moves-as-first results to the brothers of each node on the way.
        let won = |by: Color| if (by == Color::Black) == black_wins { 1.0 } else { 0.0 };
        for (depth, index) in self.path.iter().copied().enumerate().rev() {
            let mover = if depth % 2 == 0 { root_color.opposite() } else { root_color };
            let to_play = mover.opposite();
            let current = &mut self.nodes[index];
            current.visits += 1.0;
            current.wins += won(mover);
            current.simulations += 1;
            let children = current.children();
            for child in &mut self.nodes[children] {
                let at = usize::from(child.point);
                if at != PASS && self.first.ply[at] >= depth as i16 && self.first.color[at] == to_play {
                    child.rave_visits += 1.0;
                    child.rave_wins += won(to_play);
                }
            }
        }
    }

    /// Adds the visits and the wins of the moves at the root to the statistics.
    fn collect(&self, stats: &mut RootStats) {
        for (index, child) in self.nodes[self.nodes[0].children()].iter().enumerate() {
            stats.visits[index] += f64::from(child.visits);
            stats.wins[index] += f64::from(child.wins);
        }
    }

    /// The child to follow: the best by its results, mixed with the all-moves-as-first results while they are few.
    fn select(&self, parent: usize) -> usize {
        let children = self.nodes[parent].children();
        let mut best = children.start;
        let mut best_value = -1.0;
        for index in children {
            let child = &self.nodes[index];
            let mut value = child.wins / child.visits;
            if child.rave_visits > 0.0 {
                let beta = child.rave_visits / (child.visits + child.rave_visits + child.visits * child.rave_visits / EQUIVALENCE);
                value = (1.0 - beta) * value + beta * (child.rave_wins / child.rave_visits);
            }
            if value > best_value {
                best_value = value;
                best = index;
            }
        }
        best
    }

    fn expand(&mut self, index: usize, board: &Board) {
        if self.nodes.len() + board.empty_points().len() + 1 > MAX_NODES {
            return;
        }
        let me = board.to_play();
        let first = self.nodes.len();
        let moves = board.empty_points().iter().map(|point| Point::from(*point)).filter(|point| board.is_legal(*point, me) && !board.is_eye(*point, me));
        self.nodes.extend(moves.map(|point| node_with_prior(board, point, me, self.playout.shapes)));
        if self.nodes.len() == first {
            self.nodes.push(node_with_prior(board, PASS, me, self.playout.shapes));
        }
        let count = self.nodes.len() - first;
        let node = &mut self.nodes[index];
        node.first_child = first as u32;
        node.child_count = count as u16;
    }
}

/// A 4-4, 3-4 or 3-3 point in an empty corner, while there are only a few stones on a board of 13 or more.
fn opening_move(board: &Board, seed: u64) -> Point {
    let size = board.size();
    if size < 13 || board.empty_points().len() + 3 < size * size {
        return PASS;
    }
    let mut choices = Vec::new();
    for (right, bottom) in [(false, false), (false, true), (true, false), (true, true)] {
        if !is_empty_square(board, if right { size - 7 } else { 0 }, if bottom { size - 7 } else { 0 }, 7) {
            continue;
        }
        let at = |x: usize, y: usize| point_at(if right { size - 1 - x } else { x }, if bottom { size - 1 - y } else { y });
        choices.extend([(at(3, 3), 4), (at(2, 3), 2), (at(3, 2), 2), (at(2, 2), 1)]);
    }
    let total: usize = choices.iter().map(|(_, weight)| weight).sum();
    if total == 0 {
        return PASS;
    }
    let mut pick = Random::new(seed ^ board.hash()).below(total);
    for (point, weight) in choices {
        if pick < weight {
            return point;
        }
        pick -= weight;
    }
    PASS
}

/// The move for the side to move; `history` holds the earlier positions, for superko.
pub fn search(board: &Board, history: &[u64], komi: f64, limits: &SearchLimits, stop: &AtomicBool) -> SearchResult {
    let deadline = Instant::now() + Duration::from_millis(limits.milliseconds);
    let me = board.to_play();
    let mut result = SearchResult { point: PASS, resign: false, win_rate: 0.5, playouts: 0 };

    // The moves at the root: legal, superko included, and not into one's own eyes.
    let brings_back = |point: Point| {
        let mut next = board.clone();
        next.play(point, me, None);
        history.contains(&next.hash())
    };
    let moves: Vec<Point> = board
        .empty_points()
        .iter()
        .map(|point| Point::from(*point))
        .filter(|point| board.is_legal(*point, me) && !board.is_eye(*point, me) && !brings_back(*point))
        .collect();
    if moves.is_empty() {
        return result; // a pass
    }

    // On the big boards the first stones go to the empty corners, which the playouts do not see the worth of.
    let opening = opening_move(board, limits.seed);
    if opening != PASS && moves.contains(&opening) {
        result.point = opening;
        return result;
    }

    // Independent trees, one for each thread, their visits added up at the end.
    let threads = limits.threads.clamp(1, 16);
    let share = (limits.playouts / threads).max(1);
    let work = |index: usize| {
        let mut stats = RootStats::new(moves.len());
        let seed = limits.seed.wrapping_mul(7919).wrapping_add(index as u64 * 104_729).wrapping_add(1);
        let mut tree = Tree::new(board, &moves, komi, seed);
        for n in 0..share {
            if n % 16 == 0 && (stop.load(Ordering::Relaxed) || Instant::now() >= deadline) {
                break;
            }
            tree.simulate(&mut stats);
        }
        tree.collect(&mut stats);
        stats
    };
    let work = &work;
    let total = std::thread::scope(|scope| {
        let helpers: Vec<_> = (1..threads).map(|index| scope.spawn(move || work(index))).collect();
        let mut total = work(0);
        for helper in helpers {
            total.add(&helper.join().expect("a search thread does not panic"));
        }
        total
    });

    // The move most visited, the first of them.
    let best = (1..moves.len()).fold(0, |best, index| if total.visits[index] > total.visits[best] { index } else { best });
    result.point = moves[best];
    result.win_rate = total.wins[best] / total.visits[best].max(1.0);
    result.playouts = total.playouts;

    if total.playouts > 0 {
        // With every point settled, one way or the other or for neither, there is nothing left to play for: pass.
        let n = total.playouts as f64;
        let settled = |point: Point| {
            let (black, white) = (f64::from(total.black[point]) / n, f64::from(total.white[point]) / n);
            black > 0.85 || white > 0.85 || black + white < 0.15
        };
        if board.points().all(settled) {
            result.point = PASS;
        }

        // The other side has passed: if the game ending now would be won, with the stones that the playouts
        // mostly lose taken off as dead, pass too.
        if board.passes() > 0 {
            let dead: PointSet = std::array::from_fn(|point| match board.at(point) {
                Cell::Black => f64::from(total.white[point]) > n / 2.0,
                Cell::White => f64::from(total.black[point]) > n / 2.0,
                _ => false,
            });
            let owners = area_owners(board, &dead);
            let points = |color: Cell| owners.iter().filter(|owner| **owner == color).count() as f64;
            let margin = points(Cell::Black) - points(Cell::White) - komi;
            if (me == Color::Black) == (margin > 0.0) {
                result.point = PASS;
            }
        }
    }
    // Hopeless, with enough of the game played to be sure.
    let area = board.size() * board.size();
    if limits.may_resign && total.playouts >= 2000 && result.win_rate < 0.06 && board.empty_points().len() < area * 2 / 3 {
        result.resign = true;
    }
    result
}

/// The position played out many times.
pub fn estimate(board: &Board, komi: f64, playouts: usize, seed: u64, stop: &AtomicBool) -> Ownership {
    let mut playout = Playout::new(seed);
    let (mut black, mut white) = ([0u32; POINT_COUNT], [0u32; POINT_COUNT]);
    let mut margin = 0.0;
    let mut done = 0;
    while done < playouts && !stop.load(Ordering::Relaxed) {
        let mut copy = board.clone();
        margin += playout.run(&mut copy, komi, None, 0);
        count_owners(&copy, &mut black, &mut white);
        done += 1;
    }
    let n = done.max(1) as f32;
    // The share of Black's ownership, with the points neither owned counted half.
    let share = |point: usize| (black[point] as f32 + 0.5 * (done as f32 - (black[point] + white[point]) as f32)) / n;
    Ownership { black: std::array::from_fn(share), margin: margin / f64::from(n) }
}

/// Runs a computation on a thread of its own; the window polls for the result.
pub struct Worker<T> {
    thread: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    result: Arc<Mutex<Option<T>>>,
    busy: Arc<AtomicBool>,
}

impl<T> Default for Worker<T> {
    fn default() -> Worker<T> {
        Worker { thread: None, stop: Arc::default(), result: Arc::default(), busy: Arc::default() }
    }
}

impl<T> Worker<T> {
    pub fn cancel(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.stop.store(true, Ordering::Relaxed);
            let _ = thread.join();
        }
        self.busy.store(false, Ordering::Relaxed);
        self.take();
    }

    pub fn busy(&self) -> bool {
        self.busy.load(Ordering::Relaxed)
    }

    pub fn take(&mut self) -> Option<T> {
        self.result.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

impl<T: Send + 'static> Worker<T> {
    /// Starts a computation, which is to stop early once the flag it is given is set.
    pub fn start(&mut self, function: impl FnOnce(&AtomicBool) -> T + Send + 'static) {
        self.cancel();
        self.busy.store(true, Ordering::Relaxed);
        self.stop = Arc::default();
        let (stop, result, busy) = (self.stop.clone(), self.result.clone(), self.busy.clone());
        self.thread = Some(std::thread::spawn(move || {
            let found = function(&stop);
            if !stop.load(Ordering::Relaxed) {
                *result.lock().unwrap_or_else(PoisonError::into_inner) = Some(found);
            }
            busy.store(false, Ordering::Relaxed);
        }));
    }
}

impl<T> Drop for Worker<T> {
    fn drop(&mut self) {
        self.cancel();
    }
}
