//! The rules of Go on boards of 9×9 to 19×19: the board with its strings of stones and their liberties, kept up
//! to date move by move; legality with ko and superko; area scoring; and a game with its record in SGF.

use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    Black,
    White,
}

impl Color {
    pub const fn opposite(self) -> Color {
        match self {
            Color::Black => Color::White,
            Color::White => Color::Black,
        }
    }

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn cell(self) -> Cell {
        match self {
            Color::Black => Cell::Black,
            Color::White => Cell::White,
        }
    }
}

/// What lies on a point; the border round the board is a point of its own kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cell {
    Empty,
    Black,
    White,
    Border,
}

impl Cell {
    pub const ALL: [Cell; 4] = [Cell::Empty, Cell::Black, Cell::White, Cell::Border];

    const fn is_stone(self) -> bool {
        matches!(self, Cell::Black | Cell::White)
    }
}

pub const MAX_SIZE: usize = 19;
pub const STRIDE: usize = MAX_SIZE + 2;
pub const POINT_COUNT: usize = STRIDE * STRIDE;

/// A point: (y + 1) * STRIDE + x + 1, for the column x from the left and the row y from the top, as in SGF.
pub type Point = usize;
/// A corner of the border, never a point of the board.
pub const PASS: Point = 0;
/// In a game record.
pub const RESIGNATION: Point = usize::MAX;

pub const fn point_at(x: usize, y: usize) -> Point {
    (y + 1) * STRIDE + x + 1
}

pub const fn x_of(point: Point) -> usize {
    point % STRIDE - 1
}

pub const fn y_of(point: Point) -> usize {
    point / STRIDE - 1
}

/// The points next to a point of the board along the lines.
pub const fn neighbours(point: Point) -> [Point; 4] {
    [point - STRIDE, point - 1, point + 1, point + STRIDE]
}

/// The points diagonally next to a point of the board.
pub const fn diagonals(point: Point) -> [Point; 4] {
    [point - STRIDE - 1, point - STRIDE + 1, point + STRIDE - 1, point + STRIDE + 1]
}

/// The columns' letters: no I, as on Go boards.
pub const COLUMNS: &str = "ABCDEFGHJKLMNOPQRST";

/// "D4": the column's letter, I left out, and the row counted from the bottom; "pass".
pub fn point_name(point: Point, size: usize) -> String {
    match point {
        PASS => "pass".to_owned(),
        RESIGNATION => "resign".to_owned(),
        _ => format!("{}{}", COLUMNS.as_bytes()[x_of(point)] as char, size - y_of(point)),
    }
}

pub const fn max_handicap(size: usize) -> usize {
    if size >= 13 { 9 } else { 5 }
}

/// The points where handicap stones go, in the order they are placed; none on a board too small for them.
pub fn handicap_points(size: usize, stones: usize) -> Vec<Point> {
    if size < 7 {
        return Vec::new();
    }
    let edge = if size >= 13 { 3 } else { 2 };
    let (far, middle) = (size - 1 - edge, size / 2);
    // The corners, upper right and lower left first, then the middle and the sides.
    let mut points = vec![point_at(far, edge), point_at(edge, far), point_at(far, far), point_at(edge, edge)];
    if stones >= 6 {
        points.extend([point_at(edge, middle), point_at(far, middle)]);
    }
    if stones >= 8 {
        points.extend([point_at(middle, edge), point_at(middle, far)]);
    }
    if stones % 2 == 1 {
        points.push(point_at(middle, middle));
    }
    points.truncate(stones);
    points
}

const fn split_mix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A random number for each color on each point, for the hash of a position.
static ZOBRIST: [[u64; POINT_COUNT]; 2] = {
    let mut keys = [[0; POINT_COUNT]; 2];
    let mut state = 0x5EED;
    let mut color = 0;
    while color < 2 {
        let mut point = 0;
        while point < POINT_COUNT {
            keys[color][point] = split_mix(&mut state);
            point += 1;
        }
        color += 1;
    }
    keys
};

fn key_of(point: Point, color: Color) -> u64 {
    ZOBRIST[color.index()][point]
}

// Board ------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct Board {
    size: usize,
    cells: [Cell; POINT_COUNT],
    // The string of stones through a point: its first stone, and the stones in a ring.
    head: [u16; POINT_COUNT],
    next: [u16; POINT_COUNT],
    // For each string, kept at its head: its stones and its pseudo-liberties, an empty neighbour counted once
    // for every stone it touches, with their sum and the sum of their squares: when all are the same point,
    // the string is in atari.
    stones: [u16; POINT_COUNT],
    libs: [u16; POINT_COUNT],
    lib_sum: [u32; POINT_COUNT],
    lib_sum_squares: [u32; POINT_COUNT],
    empty: [u16; MAX_SIZE * MAX_SIZE],
    empty_index: [u16; POINT_COUNT],
    empty_count: usize,
    to_play: Color,
    ko: Point,
    last_move: Point,
    passes: u32,
    captures: [u32; 2],
    hash: u64,
}

impl Board {
    pub fn new(size: usize) -> Board {
        let mut board = Board {
            size: size.clamp(2, MAX_SIZE),
            cells: [Cell::Border; POINT_COUNT],
            head: [0; POINT_COUNT],
            next: [0; POINT_COUNT],
            stones: [0; POINT_COUNT],
            libs: [0; POINT_COUNT],
            lib_sum: [0; POINT_COUNT],
            lib_sum_squares: [0; POINT_COUNT],
            empty: [0; MAX_SIZE * MAX_SIZE],
            empty_index: [0; POINT_COUNT],
            empty_count: 0,
            to_play: Color::Black,
            ko: PASS,
            last_move: PASS,
            passes: 0,
            captures: [0; 2],
            hash: 0,
        };
        for point in board.points() {
            board.cells[point] = Cell::Empty;
            board.add_empty(point);
        }
        board
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// The points of the board, row by row from the top.
    pub fn points(&self) -> impl Iterator<Item = Point> + use<> {
        let size = self.size;
        (0..size).flat_map(move |y| (0..size).map(move |x| point_at(x, y)))
    }

    pub fn at(&self, point: Point) -> Cell {
        self.cells[point]
    }

    pub fn is_stone(&self, point: Point) -> bool {
        self.cells[point].is_stone()
    }

    pub fn to_play(&self) -> Color {
        self.to_play
    }

    pub fn set_to_play(&mut self, color: Color) {
        self.to_play = color;
    }

    /// The point that simple ko forbids, or `PASS`.
    pub fn ko(&self) -> Point {
        self.ko
    }

    pub fn last_move(&self) -> Point {
        self.last_move
    }

    /// The passes in a row at the end of the moves so far.
    pub fn passes(&self) -> u32 {
        self.passes
    }

    pub fn captures(&self, by: Color) -> u32 {
        self.captures[by.index()]
    }

    /// The hash of the stones alone.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    pub fn empty_points(&self) -> &[u16] {
        &self.empty[..self.empty_count]
    }

    pub fn is_on_board(&self, point: Point) -> bool {
        point > 0 && point < POINT_COUNT && self.cells[point] != Cell::Border
    }

    /// The first stone of the string through a point.
    pub fn head(&self, point: Point) -> Point {
        usize::from(self.head[point])
    }

    /// The next stone in the ring of a string.
    pub fn next_stone(&self, point: Point) -> Point {
        usize::from(self.next[point])
    }

    pub fn stone_count(&self, point: Point) -> usize {
        usize::from(self.stones[self.head(point)])
    }

    /// Whether the string through a point has one liberty left.
    pub fn in_atari(&self, point: Point) -> bool {
        let head = self.head(point);
        let (libs, sum) = (u64::from(self.libs[head]), u64::from(self.lib_sum[head]));
        libs > 0 && libs * u64::from(self.lib_sum_squares[head]) == sum * sum
    }

    /// The liberty of a string in atari.
    pub fn atari_liberty(&self, point: Point) -> Point {
        let head = self.head(point);
        (self.lib_sum[head] / u32::from(self.libs[head])) as Point
    }

    /// The liberties of the string through a point, as many as `found` holds at most; the number found.
    pub fn liberties(&self, point: Point, found: &mut [Point]) -> usize {
        let mut count = 0;
        let head = self.head(point);
        let mut stone = head;
        loop {
            for next in neighbours(stone) {
                if self.cells[next] == Cell::Empty && !found[..count].contains(&next) {
                    found[count] = next;
                    count += 1;
                    if count == found.len() {
                        return count;
                    }
                }
            }
            stone = self.next_stone(stone);
            if stone == head {
                return count;
            }
        }
    }

    /// Legal by the rules but superko: on an empty point, not retaking a ko at once, and not suicide.
    pub fn is_legal(&self, point: Point, color: Color) -> bool {
        if point == PASS {
            return true;
        }
        if !self.is_on_board(point) || self.cells[point] != Cell::Empty || point == self.ko {
            return false;
        }
        let own = color.cell();
        neighbours(point).into_iter().any(|next| match self.cells[next] {
            Cell::Empty => true,
            Cell::Border => false,
            // A string of one's own with another liberty, or one of the other side's that the move takes.
            cell => (cell == own) != self.in_atari(next),
        })
    }

    /// An eye of `color` that playouts leave alone: surrounded by its stones, the corners mostly too.
    pub fn is_eye(&self, point: Point, color: Color) -> bool {
        let (own, other) = (color.cell(), color.opposite().cell());
        let mut edge = false;
        for next in neighbours(point) {
            match self.cells[next] {
                Cell::Border => edge = true,
                cell if cell != own => return false,
                _ => {}
            }
        }
        let enemies = diagonals(point).into_iter().filter(|corner| self.cells[*corner] == other).count();
        enemies < if edge { 1 } else { 2 }
    }

    /// Whether the move would leave its string with a single liberty, and takes nothing.
    pub fn is_self_atari(&self, point: Point, color: Color) -> bool {
        let own = color.cell();
        let mut liberties = [PASS; 2];
        let mut count = 0;
        // Notes a liberty of the string the stone would make; true once there are two.
        let mut add = |liberty: Point| {
            if liberty != point && !liberties[..count].contains(&liberty) {
                liberties[count] = liberty;
                count += 1;
            }
            count >= 2
        };
        for next in neighbours(point) {
            if self.cells[next] == Cell::Empty && add(next) {
                return false;
            }
        }
        for next in neighbours(point) {
            let cell = self.cells[next];
            if !cell.is_stone() {
                continue;
            }
            if cell != own {
                if self.in_atari(next) {
                    return false; // it takes the string, and gains a liberty where it stood
                }
                continue;
            }
            let mut found = [PASS; 3];
            let libs = self.liberties(next, &mut found);
            if found[..libs].iter().any(|liberty| add(*liberty)) {
                return false;
            }
        }
        true
    }

    /// A legal move or a pass; the stones taken are added to `captured` when given.
    pub fn play(&mut self, point: Point, color: Color, mut captured: Option<&mut Vec<Point>>) {
        self.ko = PASS;
        self.to_play = color.opposite();
        self.last_move = point;
        if point == PASS {
            self.passes += 1;
            return;
        }
        self.passes = 0;

        let (own, other) = (color.cell(), color.opposite().cell());
        self.remove_empty(point);
        self.cells[point] = own;
        self.hash ^= key_of(point, color);
        self.head[point] = point as u16;
        self.next[point] = point as u16;
        self.stones[point] = 1;
        self.libs[point] = 0;
        self.lib_sum[point] = 0;
        self.lib_sum_squares[point] = 0;
        for next in neighbours(point) {
            if self.cells[next] == Cell::Empty {
                self.add_liberty(point, next);
            } else if self.is_stone(next) {
                self.remove_liberty(self.head(next), point);
            }
        }
        for next in neighbours(point) {
            if self.cells[next] == own && self.head(next) != self.head(point) {
                self.merge(self.head(point), self.head(next));
            }
        }

        let mut taken = 0;
        let mut last_taken = PASS;
        for next in neighbours(point) {
            if self.cells[next] == other && self.libs[self.head(next)] == 0 {
                last_taken = next;
                taken += self.remove_string(self.head(next), captured.as_deref_mut());
            }
        }
        self.captures[color.index()] += taken;
        // One stone taken by a lone stone that is left with that point as its only liberty: a ko.
        let head = self.head(point);
        if taken == 1 && self.stones[head] == 1 && self.libs[head] == 1 {
            self.ko = last_taken;
        }
    }

    /// A stone set on the board before the game, such as a handicap stone.
    pub fn setup(&mut self, point: Point, color: Color) {
        let to_play = self.to_play;
        self.play(point, color, None);
        self.to_play = to_play;
        self.last_move = PASS;
        self.ko = PASS;
    }

    /// Black's points less White's, counting stones and the empty points next only to stones of one color:
    /// the score at the end of a playout, where every empty point is a single eye or a shared liberty.
    pub fn quick_score(&self) -> i32 {
        let worth = |point| match self.settled_owner(point) {
            Cell::Black => 1,
            Cell::White => -1,
            _ => 0,
        };
        self.points().map(worth).sum()
    }

    /// Who owns a point at the end of a playout: its stone's color, or the color of all the stones round an
    /// empty point; `Cell::Empty` for neither.
    pub fn settled_owner(&self, point: Point) -> Cell {
        let cell = self.cells[point];
        if cell != Cell::Empty {
            return cell;
        }
        let around = neighbours(point).map(|next| self.cells[next]);
        match (around.contains(&Cell::Black), around.contains(&Cell::White)) {
            (true, false) => Cell::Black,
            (false, true) => Cell::White,
            _ => Cell::Empty,
        }
    }

    fn add_empty(&mut self, point: Point) {
        self.empty_index[point] = self.empty_count as u16;
        self.empty[self.empty_count] = point as u16;
        self.empty_count += 1;
    }

    fn remove_empty(&mut self, point: Point) {
        let index = usize::from(self.empty_index[point]);
        self.empty_count -= 1;
        let last = self.empty[self.empty_count];
        self.empty[index] = last;
        self.empty_index[usize::from(last)] = index as u16;
    }

    fn add_liberty(&mut self, head: Point, liberty: Point) {
        self.libs[head] += 1;
        self.lib_sum[head] += liberty as u32;
        self.lib_sum_squares[head] += (liberty * liberty) as u32;
    }

    fn remove_liberty(&mut self, head: Point, liberty: Point) {
        self.libs[head] -= 1;
        self.lib_sum[head] -= liberty as u32;
        self.lib_sum_squares[head] -= (liberty * liberty) as u32;
    }

    /// Joins two strings, given by their heads, into one: the smaller into the larger.
    fn merge(&mut self, a: Point, b: Point) {
        let (to, from) = if self.stones[a] < self.stones[b] { (b, a) } else { (a, b) };
        let mut stone = from;
        loop {
            self.head[stone] = to as u16;
            stone = self.next_stone(stone);
            if stone == from {
                break;
            }
        }
        self.next.swap(to, from);
        self.stones[to] += self.stones[from];
        self.libs[to] += self.libs[from];
        self.lib_sum[to] += self.lib_sum[from];
        self.lib_sum_squares[to] += self.lib_sum_squares[from];
    }

    /// Takes a string off the board; the number of its stones.
    fn remove_string(&mut self, head: Point, mut captured: Option<&mut Vec<Point>>) -> u32 {
        let color = if self.cells[head] == Cell::Black { Color::Black } else { Color::White };
        let mut count = 0;
        let mut stone = head;
        loop {
            self.cells[stone] = Cell::Empty;
            self.hash ^= key_of(stone, color);
            self.add_empty(stone);
            if let Some(captured) = captured.as_deref_mut() {
                captured.push(stone);
            }
            count += 1;
            stone = self.next_stone(stone);
            if stone == head {
                break;
            }
        }
        // The points freed are liberties again of the strings round them.
        loop {
            for next in neighbours(stone) {
                if self.is_stone(next) {
                    self.add_liberty(self.head(next), stone);
                }
            }
            stone = self.next_stone(stone);
            if stone == head {
                break;
            }
        }
        count
    }
}

// Game -------------------------------------------------------------------------------------------

pub type PointSet = [bool; POINT_COUNT];

/// The owner of every point with the dead stones taken off: the stones' color, the color that alone surrounds an
/// empty region, or `Cell::Empty` for neither.
pub fn area_owners(board: &Board, dead: &PointSet) -> [Cell; POINT_COUNT] {
    let cells: [Cell; POINT_COUNT] = std::array::from_fn(|point| if dead[point] { Cell::Empty } else { board.at(point) });
    let mut owner = [Cell::Empty; POINT_COUNT];
    let mut seen = [false; POINT_COUNT];
    let (mut region, mut stack) = (Vec::new(), Vec::new());
    for point in 0..POINT_COUNT {
        let cell = cells[point];
        if cell.is_stone() {
            owner[point] = cell;
            continue;
        }
        if cell != Cell::Empty || seen[point] {
            continue;
        }
        // An empty region: whose stones it touches.
        let (mut black, mut white) = (false, false);
        region.clear();
        stack.push(point);
        seen[point] = true;
        while let Some(at) = stack.pop() {
            region.push(at);
            for next in neighbours(at) {
                let around = cells[next];
                black |= around == Cell::Black;
                white |= around == Cell::White;
                if around == Cell::Empty && !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        if black != white {
            for at in &region {
                owner[*at] = if black { Cell::Black } else { Cell::White };
            }
        }
    }
    owner
}

/// Area scoring: stones and the territory they surround; komi goes to White.
#[derive(Clone, Copy)]
pub struct Score {
    pub black: i32,
    pub white: i32,
    pub komi: f64,
}

impl Score {
    /// Above zero, Black wins.
    pub fn margin(&self) -> f64 {
        f64::from(self.black - self.white) - self.komi
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Playing,
    Scoring,
    Finished,
}

#[derive(Clone)]
pub struct MoveRecord {
    pub point: Point, // a stone, `PASS` or `RESIGNATION`
    pub color: Color,
    pub captured: Vec<Point>,
}

impl MoveRecord {
    pub fn is_pass(&self) -> bool {
        self.point == PASS
    }

    pub fn is_resignation(&self) -> bool {
        self.point == RESIGNATION
    }
}

/// A game: the size, the handicap and komi, the moves played and those taken back, and at the end the dead
/// stones the players agree on.
pub struct Game {
    handicap: usize,
    komi: f64,
    positions: Vec<Board>,
    hashes: Vec<u64>,
    moves: Vec<MoveRecord>,
    redo: Vec<MoveRecord>, // the next to replay at the end
    dead: PointSet,
    accepted: bool,
}

impl Game {
    /// A game from its first position. Komi is 7.5, or 0.5 with a handicap.
    pub fn new(size: usize, handicap: usize) -> Game {
        let mut start = Board::new(size);
        let stones = if handicap >= 2 { handicap_points(start.size(), handicap.min(max_handicap(start.size()))) } else { Vec::new() };
        for point in &stones {
            start.setup(*point, Color::Black);
        }
        if !stones.is_empty() {
            start.set_to_play(Color::White);
        }
        Game::begin(start, stones.len(), if stones.is_empty() { 7.5 } else { 0.5 })
    }

    fn begin(start: Board, handicap: usize, komi: f64) -> Game {
        Game {
            handicap,
            komi,
            hashes: vec![start.hash()],
            positions: vec![start],
            moves: Vec::new(),
            redo: Vec::new(),
            dead: [false; POINT_COUNT],
            accepted: false,
        }
    }

    pub fn size(&self) -> usize {
        self.start().size()
    }

    pub fn handicap(&self) -> usize {
        self.handicap
    }

    pub fn komi(&self) -> f64 {
        self.komi
    }

    pub fn current(&self) -> &Board {
        self.positions.last().expect("a game has its first position")
    }

    pub fn start(&self) -> &Board {
        &self.positions[0]
    }

    pub fn moves(&self) -> &[MoveRecord] {
        &self.moves
    }

    pub fn ply(&self) -> usize {
        self.moves.len()
    }

    pub fn phase(&self) -> Phase {
        if self.resigned() {
            Phase::Finished
        } else if self.current().passes() < 2 {
            Phase::Playing
        } else if self.accepted {
            Phase::Finished
        } else {
            Phase::Scoring
        }
    }

    /// Whether the game ended with a resignation.
    pub fn resigned(&self) -> bool {
        self.moves.last().is_some_and(MoveRecord::is_resignation)
    }

    /// The earlier positions, for superko.
    pub fn history(&self) -> &[u64] {
        &self.hashes
    }

    /// Legal with positional superko: no move may bring back an earlier position.
    pub fn is_legal(&self, point: Point) -> bool {
        if self.phase() != Phase::Playing {
            return false;
        }
        if point == PASS {
            return true;
        }
        let board = self.current();
        if !board.is_legal(point, board.to_play()) {
            return false;
        }
        let mut next = board.clone();
        next.play(point, board.to_play(), None);
        !self.hashes.contains(&next.hash())
    }

    fn push(&mut self, point: Point, color: Color) {
        let mut record = MoveRecord { point, color, captured: Vec::new() };
        let mut next = self.current().clone();
        if point != RESIGNATION {
            next.play(point, color, Some(&mut record.captured));
        }
        self.hashes.push(next.hash());
        self.positions.push(next);
        self.moves.push(record);
        self.dead = [false; POINT_COUNT];
        self.accepted = false;
    }

    /// A stone or a pass; after two passes the game goes to scoring.
    pub fn play(&mut self, point: Point) -> bool {
        if !self.is_legal(point) {
            return false;
        }
        self.push(point, self.current().to_play());
        self.redo.clear();
        true
    }

    pub fn resign(&mut self, who: Color) {
        if self.phase() != Phase::Finished {
            self.push(RESIGNATION, who);
            self.redo.clear();
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.moves.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(record) = self.moves.pop() else { return false };
        self.redo.push(record);
        self.positions.pop();
        self.hashes.pop();
        self.dead = [false; POINT_COUNT];
        self.accepted = false;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(record) = self.redo.pop() else { return false };
        self.push(record.point, record.color);
        true
    }

    /// The dead stones, marked string by string while the game is counted.
    pub fn dead(&self) -> &PointSet {
        &self.dead
    }

    pub fn set_dead(&mut self, dead: &PointSet) {
        let board = self.positions.last().expect("a game has its first position");
        self.dead = std::array::from_fn(|point| dead[point] && board.is_stone(point));
    }

    pub fn toggle_dead(&mut self, point: Point) {
        let board = self.positions.last().expect("a game has its first position");
        if !board.is_stone(point) {
            return;
        }
        // The stones of the color clicked in the area they share: reached through their own stones and empty points.
        let color = board.at(point);
        let dead = !self.dead[point];
        let mut seen = [false; POINT_COUNT];
        let mut stack = vec![point];
        seen[point] = true;
        while let Some(at) = stack.pop() {
            if board.at(at) == color {
                self.dead[at] = dead;
            }
            for next in neighbours(at) {
                let cell = board.at(next);
                if !seen[next] && (cell == color || cell == Cell::Empty) {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
    }

    /// The owner of every point once the dead stones are off: Black, White or `Cell::Empty` for neither.
    pub fn territory(&self) -> [Cell; POINT_COUNT] {
        area_owners(self.current(), &self.dead)
    }

    /// The score with the dead stones taken off.
    pub fn count(&self) -> Score {
        let owner = self.territory();
        let points = |color: Cell| owner.iter().filter(|cell| **cell == color).count() as i32;
        Score { black: points(Cell::Black), white: points(Cell::White), komi: self.komi }
    }

    pub fn accept(&mut self) {
        self.accepted = true;
    }

    /// Black's margin after scoring or resignation: positive for Black; nothing while the game goes on.
    pub fn margin(&self) -> Option<f64> {
        if self.phase() != Phase::Finished {
            return None;
        }
        Some(match self.moves.last() {
            Some(last) if last.is_resignation() => {
                if last.color == Color::Black {
                    -1000.0
                } else {
                    1000.0
                }
            }
            _ => self.count().margin(),
        })
    }

    pub fn winner(&self) -> Option<Color> {
        match self.margin()? {
            margin if margin > 0.0 => Some(Color::Black),
            margin if margin < 0.0 => Some(Color::White),
            _ => None,
        }
    }

    /// "B+3.5", "W+R"; empty while the game goes on.
    pub fn result_text(&self) -> String {
        let Some(margin) = self.margin() else { return String::new() };
        if margin == 0.0 {
            return "0".to_owned();
        }
        let winner = if margin > 0.0 { 'B' } else { 'W' };
        if self.resigned() { format!("{winner}+R") } else { format!("{winner}+{}", margin.abs()) }
    }

    // SGF ----------------------------------------------------------------------------------------

    pub fn to_sgf(&self, black_name: &str, white_name: &str) -> String {
        let (year, month, day) = today();
        let mut sgf = format!("(;GM[1]FF[4]CA[UTF-8]AP[3D Go:1.0]SZ[{}]KM[{}]RU[Chinese]DT[{year:04}-{month:02}-{day:02}]", self.size(), self.komi);
        if !black_name.is_empty() {
            sgf += &format!("PB[{}]", escape(black_name));
        }
        if !white_name.is_empty() {
            sgf += &format!("PW[{}]", escape(white_name));
        }
        if self.handicap > 0 {
            sgf += &format!("HA[{}]", self.handicap);
        }
        // The stones on the board at the start.
        let start = self.start();
        for (property, color) in [("AB", Cell::Black), ("AW", Cell::White)] {
            let stones: String = start.points().filter(|point| start.at(*point) == color).map(|point| format!("[{}]", sgf_point(point))).collect();
            if !stones.is_empty() {
                sgf += property;
                sgf += &stones;
            }
        }
        if start.to_play() == Color::White {
            sgf += "PL[W]";
        }
        let result = self.result_text();
        if !result.is_empty() {
            sgf += &format!("RE[{result}]");
        }
        let stones_and_passes = self.moves.iter().filter(|record| !record.is_resignation());
        for (number, record) in stones_and_passes.enumerate() {
            let line_break = if (number + 1) % 10 == 0 { "\n" } else { "" };
            let color = if record.color == Color::Black { 'B' } else { 'W' };
            write!(sgf, "{line_break};{color}[{}]", sgf_point(record.point)).expect("writing to a string does not fail");
        }
        sgf + ")\n"
    }

    /// An SGF game: its main line, with the size, komi, handicap and setup stones.
    pub fn from_sgf(text: &str) -> Result<Game, String> {
        // Whatever comes before the game, such as a message it was pasted from, is skipped.
        let begin = text.find("(;").ok_or("no SGF game")?;
        let nodes = SgfReader::new(&text[begin..]).main_line()?;
        let root = nodes.first().ok_or("an empty game")?;

        let size = root.value("SZ").map_or(19, leading_integer);
        if !(2..=MAX_SIZE as i64).contains(&size) {
            return Err(format!("a board of {size} is not supported"));
        }
        let size = size as usize;
        let handicap = root.value("HA").map_or(0, leading_integer).clamp(0, 9) as usize;
        let komi = root.value("KM").map_or(if handicap >= 2 { 0.5 } else { 7.5 }, leading_number);

        // The setup stones, or the usual handicap when none are given.
        let mut start = Board::new(size);
        let mut setup = false;
        for (name, color) in [("AB", Color::Black), ("AW", Color::White)] {
            for value in root.values(name) {
                for point in read_points(value, size) {
                    if start.at(point) == Cell::Empty && start.is_legal(point, color) {
                        start.setup(point, color);
                        setup = true;
                    }
                }
            }
        }
        if !setup && handicap >= 2 {
            for point in handicap_points(size, handicap) {
                start.setup(point, Color::Black);
            }
        }
        if handicap >= 2 {
            start.set_to_play(Color::White);
        }
        if let Some(player) = root.value("PL") {
            start.set_to_play(if player.eq_ignore_ascii_case("W") { Color::White } else { Color::Black });
        }

        let mut game = Game::begin(start, handicap, komi);
        let mut number = 0;
        for node in &nodes {
            for (name, color) in [("B", Color::Black), ("W", Color::White)] {
                let Some(value) = node.value(name) else { continue };
                number += 1;
                if game.phase() != Phase::Playing {
                    return Err(format!("move {number} comes after the end"));
                }
                // A move out of turn: the other side passed.
                if game.current().to_play() != color {
                    game.push(PASS, color.opposite());
                }
                // No point, or "tt" as in the older files, is a pass.
                let point = if value.is_empty() || value == "tt" {
                    PASS
                } else {
                    match read_points(value, size)[..] {
                        [point] => point,
                        _ => return Err(format!("move {number} is not a point")),
                    }
                };
                if !game.is_legal(point) {
                    return Err(format!("move {number} ({}) is not legal", point_name(point, size)));
                }
                game.push(point, color);
            }
        }
        if let Some(result) = root.value("RE") {
            // The side that lost resigned.
            if result.len() >= 3 && &result.as_bytes()[1..3] == b"+R" && game.phase() == Phase::Playing {
                game.push(RESIGNATION, if result.as_bytes()[0].eq_ignore_ascii_case(&b'B') { Color::White } else { Color::Black });
            }
        }
        Ok(game)
    }
}

fn sgf_point(point: Point) -> String {
    if point == PASS {
        return String::new();
    }
    [x_of(point), y_of(point)].into_iter().map(|coordinate| (b'a' + coordinate as u8) as char).collect()
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if c == ']' || c == '\\' {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// The whole number a text begins with, as `atoi` reads it; 0 without one.
fn leading_integer(text: &str) -> i64 {
    let text = text.trim_start();
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    let length = digits.bytes().take_while(u8::is_ascii_digit).count();
    let value: i64 = digits[..length.min(15)].parse().unwrap_or(0);
    if text.starts_with('-') { -value } else { value }
}

/// The number a text begins with, as `atof` reads it; 0 without one.
fn leading_number(text: &str) -> f64 {
    let text = text.trim_start();
    // The longest beginning that reads as a number.
    (1..=text.len())
        .rev()
        .filter(|end| text.is_char_boundary(*end))
        .find_map(|end| text[..end].parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
}

/// "dd", or a rectangle "aa:cc" in a list of setup stones.
fn read_points(value: &str, size: usize) -> Vec<Point> {
    let one = |text: &str| match *text.as_bytes() {
        [x @ b'a'..=b'z', y @ b'a'..=b'z'] if usize::from(x.max(y) - b'a') < size => Some((usize::from(x - b'a'), usize::from(y - b'a'))),
        _ => None,
    };
    let (first, last) = value.split_once(':').unwrap_or((value, value));
    let Some(first) = one(first) else { return Vec::new() };
    let last = one(last).unwrap_or(first);
    let (columns, rows) = (first.0.min(last.0)..=first.0.max(last.0), first.1.min(last.1)..=first.1.max(last.1));
    rows.flat_map(|y| columns.clone().map(move |x| point_at(x, y))).collect()
}

struct Property {
    name: String,
    values: Vec<String>,
}

#[derive(Default)]
struct Node(Vec<Property>);

impl Node {
    fn values(&self, name: &str) -> &[String] {
        self.0.iter().find(|property| property.name == name).map_or(&[], |property| &property.values)
    }

    /// The first value of a property.
    fn value(&self, name: &str) -> Option<&str> {
        self.values(name).first().map(String::as_str)
    }
}

/// Reads the nodes of the main line: at each fork, the first variation.
struct SgfReader<'a> {
    text: &'a [u8],
    at: usize,
}

impl SgfReader<'_> {
    fn new(text: &str) -> SgfReader<'_> {
        SgfReader { text: text.as_bytes(), at: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    fn main_line(mut self) -> Result<Vec<Node>, String> {
        self.skip_space();
        if self.peek() != Some(b'(') {
            return Err("not an SGF game".to_owned());
        }
        self.at += 1;
        let mut nodes = Vec::new();
        self.sequence(&mut nodes)?;
        Ok(nodes)
    }

    fn sequence(&mut self, nodes: &mut Vec<Node>) -> Result<(), String> {
        loop {
            self.skip_space();
            // A game cut short is read as far as it goes.
            let Some(c) = self.peek() else { return Ok(()) };
            self.at += 1;
            match c {
                b';' => nodes.push(self.read_node()),
                b'(' => {
                    // The first variation is followed; the others are skipped.
                    self.sequence(nodes)?;
                    self.skip_variations();
                    return Ok(());
                }
                b')' => return Ok(()),
                _ => return Err(format!("unexpected '{}'", c as char)),
            }
        }
    }

    fn read_node(&mut self) -> Node {
        let mut node = Node::default();
        loop {
            self.skip_space();
            // The name is its capital letters: older files write "AddBlack" for "AB".
            let mut name = String::new();
            while let Some(c) = self.peek().filter(u8::is_ascii_alphabetic) {
                if c.is_ascii_uppercase() {
                    name.push(c as char);
                }
                self.at += 1;
            }
            if name.is_empty() {
                return node;
            }
            let mut property = Property { name, values: Vec::new() };
            self.skip_space();
            while self.peek() == Some(b'[') {
                self.at += 1;
                let mut value = Vec::new();
                while let Some(c) = self.peek().filter(|c| *c != b']') {
                    self.at += 1;
                    // A backslash takes the next character as it is.
                    match (c, self.peek()) {
                        (b'\\', Some(escaped)) => {
                            value.push(escaped);
                            self.at += 1;
                        }
                        _ => value.push(c),
                    }
                }
                self.at += 1;
                property.values.push(String::from_utf8_lossy(&value).into_owned());
                self.skip_space();
            }
            node.0.push(property);
        }
    }

    /// After the first variation, the rest up to the end of the enclosing sequence.
    fn skip_variations(&mut self) {
        let mut depth = 0;
        let mut in_value = false;
        while let Some(c) = self.peek() {
            self.at += 1;
            match c {
                b'\\' if in_value => self.at += 1,
                b']' if in_value => in_value = false,
                _ if in_value => {}
                b'[' => in_value = true,
                b'(' => depth += 1,
                b')' if depth == 0 => return,
                b')' => depth -= 1,
                _ => {}
            }
        }
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(|c| matches!(c, b' ' | b'\t' | b'\r' | b'\n')) {
            self.at += 1;
        }
    }
}

/// Today's date by universal time, as a year, a month and a day.
fn today() -> (i64, i64, i64) {
    // After Howard Hinnant's "civil from days".
    let days = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs() / 86_400) as i64;
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}
