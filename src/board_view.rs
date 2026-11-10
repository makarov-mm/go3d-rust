//! The stones as they lie and move: heaped in the bowls, flying from a bowl to the point played, the stones
//! taken flying into the lid of the player who took them, all of it in reverse when a move is taken back,
//! and dissolving in when a game is set out.

use crate::board_geometry::{self as board, Layout};
use crate::go::{self, Cell, Color, Game, MAX_SIZE, MoveRecord, Point, PointSet};
use crate::math::{Mat4, PI, Vec3, vec3};
use crate::renderer::{StoneDraw, mark};
use crate::stone_model::HALF_HEIGHT;

/// What happened in a frame, for the sounds.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Event {
    Place,
    Capture,
    Return,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Board(Point),
    Bowl,
    /// In a lid: whose, and the stone's place there.
    Lid {
        lid: usize,
        slot: usize,
    },
    Leaving,
}

struct Stone {
    color: Color,
    place: Place,
    position: Vec3, // the middle of the stone
    yaw: f32,
    seed: f32,
    // How it lies, leaning on the stones under it.
    tilt_axis: Vec3,
    tilt: f32,
    // A flight from one place to another, in an arc, turning from one tilt to another.
    from: Vec3,
    to: Vec3,
    from_axis: Vec3,
    from_tilt: f32,
    flight_time: f32,
    flight_duration: f32,
    flight_delay: f32,
    arc: f32,
    flying: bool,
    remove_when_landed: bool,
    landing: Event,
    dissolve: f32,
    dissolve_target: f32,
    dissolve_delay: f32,
    remove_when_gone: bool,
    // In the preview of a capture.
    lift: f32,
    lift_target: f32,
}

/// How a stone lies: where, how far it leans and about which axis.
struct Rest {
    position: Vec3,
    tilt: f32,
    axis: Vec3,
}

impl Rest {
    /// Lying flat, as on the board.
    fn flat(position: Vec3) -> Rest {
        Rest { position, tilt: 0.0, axis: Vec3::X }
    }
}

const FADE_TIME: f32 = 0.45;
const HEAP_LAYER: f32 = 0.24; // between the layers of stones in a bowl
const LID_LAYER: f32 = 0.2; // and in a lid
const SPACING: f32 = 0.86; // between the stones lying side by side, a little leaning on each other

fn ease(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn approach(value: f32, target: f32, step: f32) -> f32 {
    if value < target { (value + step).min(target) } else { (value - step).max(target) }
}

/// A number in [0, 1) that stays the same for the same arguments.
fn hash(a: usize, b: usize) -> f32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

impl Stone {
    /// Sends the stone on a flight to where it is to lie.
    fn fly(&mut self, to: Rest, duration: f32, arc: f32, delay: f32, landing: Event) {
        // A stone already on its way starts again from where it is, with the lean of the end it is nearer.
        let nearer_start = self.flying && (self.flight_delay > 0.0 || self.flight_time < self.flight_duration * 0.5);
        if !nearer_start {
            self.from_axis = self.tilt_axis;
            self.from_tilt = self.tilt;
        }
        self.from = self.position;
        self.to = to.position;
        self.tilt_axis = to.axis;
        self.tilt = to.tilt;
        self.flight_time = 0.0;
        self.flight_duration = duration;
        self.flight_delay = delay;
        self.arc = arc;
        self.flying = true;
        self.landing = landing;
    }

    fn on_board(&self) -> Option<Point> {
        match self.place {
            Place::Board(point) => Some(point),
            _ => None,
        }
    }

    /// Whether the stone is where it belongs, neither flying nor dissolving.
    fn settled(&self) -> bool {
        !self.flying && self.dissolve == self.dissolve_target
    }
}

pub struct BoardView {
    layout: Layout,
    stones: Vec<Stone>,
    lid_counts: [usize; 2],
    seed: u32,
}

impl BoardView {
    /// The view for a size of board, with the heaps of stones in the bowls.
    pub fn new(layout: &Layout) -> BoardView {
        let mut view = BoardView { layout: layout.clone(), stones: Vec::new(), lid_counts: [0; 2], seed: 1 };
        view.build_heaps();
        view
    }

    /// The places of the bowls and the lids for a size of board; the heaps in the bowls are made again.
    pub fn set_layout(&mut self, layout: &Layout) {
        self.layout = layout.clone();
        self.stones.clear();
        self.lid_counts = [0; 2];
        self.build_heaps();
    }

    fn random(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    fn random_axis(&mut self) -> Vec3 {
        let (s, c) = (self.random() * 2.0 * PI).sin_cos();
        vec3(c, 0.0, s)
    }

    /// The height of what lies under a stone in flight: the board or the table.
    fn ground_at(&self, position: Vec3) -> f32 {
        let over_board = position.x.abs() < self.layout.board_half && position.z.abs() < self.layout.board_half;
        if over_board { 0.0 } else { self.layout.table_height() }
    }

    fn on_board(&self, point: Point) -> Vec3 {
        self.layout.point_center(point) + vec3(0.0, HALF_HEIGHT, 0.0)
    }

    /// Adds a stone lying flat; one that appears dissolves in after a delay. Its place among the stones.
    fn add(&mut self, color: Color, place: Place, position: Vec3, appear: bool, delay: f32) -> usize {
        let yaw = self.random() * 2.0 * PI;
        let seed = self.random();
        self.stones.push(Stone {
            color,
            place,
            position,
            yaw,
            seed,
            tilt_axis: Vec3::X,
            tilt: 0.0,
            from: Vec3::ZERO,
            to: Vec3::ZERO,
            from_axis: Vec3::X,
            from_tilt: 0.0,
            flight_time: 0.0,
            flight_duration: 0.0,
            flight_delay: 0.0,
            arc: 0.0,
            flying: false,
            remove_when_landed: false,
            landing: Event::Place,
            dissolve: if appear { 1.0 } else { 0.0 },
            dissolve_target: 0.0,
            dissolve_delay: if appear { delay } else { 0.0 },
            remove_when_gone: false,
            lift: 0.0,
            lift_target: 0.0,
        });
        self.stones.len() - 1
    }

    /// The stone lying on a point, as its place among the stones.
    fn stone_on(&self, point: Point) -> Option<usize> {
        self.stones.iter().position(|stone| stone.place == Place::Board(point))
    }

    /// Each bowl holds a heap of its stones: a few layers, the stones leaning on each other, higher in the middle.
    fn build_heaps(&mut self) {
        let reach = 0.72 * self.layout.bowl_radius - 0.48;
        let floor = board::heap_height(self.layout.bowl_height) - 0.05; // above the foot of the bowl
        for (bowl, color) in [Color::Black, Color::White].into_iter().enumerate() {
            let base = self.layout.bowls[bowl];
            for layer in 0..3 {
                let layer_reach = reach * (1.0 - 0.32 * layer as f32);
                let shift = if layer == 1 { SPACING * 0.5 } else { 0.0 };
                let count = (layer_reach / SPACING).ceil() as i32 + 1;
                for row in -count..=count {
                    for column in -count..=count {
                        let mut x = (column as f32 + 0.5 * (row & 1) as f32) * SPACING + shift;
                        let mut z = row as f32 * SPACING * 0.866 + shift * 0.577;
                        x += (self.random() - 0.5) * 0.16;
                        z += (self.random() - 0.5) * 0.16;
                        let r2 = x * x + z * z;
                        if r2 > layer_reach * layer_reach {
                            continue;
                        }
                        let dome = 0.12 * (1.0 - r2 / (reach * reach).max(0.01));
                        let y = floor + HALF_HEIGHT + layer as f32 * HEAP_LAYER + dome;
                        let stone = self.add(color, Place::Bowl, base + vec3(x, y, z), false, 0.0);
                        self.stones[stone].tilt_axis = self.random_axis();
                        self.stones[stone].tilt = 0.1 + 0.3 * self.random();
                    }
                }
            }
        }
    }

    /// Where a stone is taken from a bowl and put back into it: the top of the heap.
    fn heap_top(&mut self, bowl: usize) -> Vec3 {
        let (s, c) = (self.random() * 2.0 * PI).sin_cos();
        let radius = 0.35 * self.random().sqrt();
        let top = board::heap_height(self.layout.bowl_height) - 0.05 + HALF_HEIGHT + 2.0 * HEAP_LAYER + 0.12;
        self.layout.bowls[bowl] + vec3(c * radius, top, s * radius)
    }

    /// The stones taken lie in the lid in a spiral, as many as fit on its floor, and then in smaller layers on top.
    fn lid_slot(&self, lid: usize, slot: usize) -> Rest {
        let reach = 0.84 * self.layout.bowl_radius - 0.5;
        let (mut index, mut layer) = (slot, 0);
        let (layer_reach, capacity) = loop {
            let layer_reach = reach * (1.0 - 0.2 * layer as f32).max(0.3);
            let capacity = ((0.85 * (layer_reach / 0.5).powi(2)) as usize).max(3);
            if index < capacity {
                break (layer_reach, capacity);
            }
            index -= capacity;
            layer += 1;
        };
        let radius = layer_reach * ((index as f32 + 0.5) / capacity as f32).sqrt() + 0.08 * (hash(lid * 977 + 1, slot) - 0.5);
        let (s, c) = (index as f32 * 2.39996 + layer as f32 * 1.3).sin_cos();
        let (axis_s, axis_c) = (2.0 * PI * hash(lid * 977 + 3, slot)).sin_cos();
        let y = board::lid_floor(self.layout.bowl_radius) + HALF_HEIGHT + layer as f32 * LID_LAYER;
        Rest {
            position: self.layout.lids[lid] + vec3(c * radius, y, s * radius),
            tilt: 0.05 + 0.15 * hash(lid * 977 + 2, slot) + if layer > 0 { 0.1 } else { 0.0 },
            axis: vec3(axis_c, 0.0, axis_s),
        }
    }

    /// Sends a stone of the board to the next free place of a lid.
    fn send_to_lid(&mut self, stone: usize, lid: usize, delay: f32) {
        let slot = self.lid_counts[lid];
        self.lid_counts[lid] += 1;
        let rest = self.lid_slot(lid, slot);
        let stone = &mut self.stones[stone];
        stone.place = Place::Lid { lid, slot };
        stone.fly(rest, 0.62, 1.5, delay, Event::Capture);
    }

    /// Sets out the game's position on the board, and the stones taken in the lids.
    pub fn rebuild(&mut self, game: &Game, animate: bool) {
        // The stones on the board and in the lids go; the heaps in the bowls stay.
        for stone in self.stones.iter_mut().filter(|stone| stone.place != Place::Bowl) {
            stone.place = Place::Leaving;
            stone.flying = false;
            stone.remove_when_landed = false;
            stone.dissolve_target = 1.0;
            stone.dissolve_delay = 0.0;
            stone.remove_when_gone = true;
        }
        if !animate {
            self.stones.retain(|stone| stone.place != Place::Leaving);
        }

        let position = game.current();
        for (order, point) in position.points().enumerate() {
            let color = match position.at(point) {
                Cell::Black => Color::Black,
                Cell::White => Color::White,
                _ => continue,
            };
            self.add(color, Place::Board(point), self.on_board(point), animate, 0.25 + 0.0035 * order as f32);
        }
        for (lid, owner) in [Color::Black, Color::White].into_iter().enumerate() {
            let count = position.captures(owner) as usize;
            for slot in 0..count {
                let rest = self.lid_slot(lid, slot);
                let stone = self.add(owner.opposite(), Place::Lid { lid, slot }, rest.position, animate, 0.4 + 0.01 * slot as f32);
                self.stones[stone].tilt = rest.tilt;
                self.stones[stone].tilt_axis = rest.axis;
            }
            self.lid_counts[lid] = count;
        }
    }

    /// A move played (forward) or taken back.
    pub fn play_move(&mut self, record: &MoveRecord, forward: bool) {
        if record.is_pass() || record.is_resignation() {
            return;
        }
        let bowl = record.color.index();
        let lid = bowl; // a player keeps the stones taken in the lid of their own bowl
        let target = self.on_board(record.point);
        // The stones taken, the nearest to the stone played first.
        let mut captured = record.captured.clone();
        captured.sort_by(|a, b| (self.on_board(*a) - target).length().total_cmp(&(self.on_board(*b) - target).length()));

        if forward {
            // A stone from the player's bowl flies to the point.
            let start = self.heap_top(bowl);
            let stone = self.add(record.color, Place::Board(record.point), start, false, 0.0);
            let axis = self.random_axis();
            let distance = (target - start).length();
            let duration = (0.4 + 0.012 * distance).min(0.68);
            let stone = &mut self.stones[stone];
            stone.tilt_axis = axis;
            stone.tilt = 0.3;
            stone.fly(Rest::flat(target), duration, 0.9 + 0.035 * distance, 0.0, Event::Place);

            // Then the stones it takes fly into the lid.
            let mut delay = duration + 0.08;
            for point in captured {
                if let Some(taken) = self.stone_on(point) {
                    self.send_to_lid(taken, lid, delay);
                    delay += 0.045;
                }
            }
            return;
        }

        // Back: the stone returns to its bowl, and the stones it took come back from the lid, the last put there first.
        if let Some(stone) = self.stone_on(record.point) {
            let heap = Rest { position: self.heap_top(bowl), tilt: 0.3, axis: self.random_axis() };
            let stone = &mut self.stones[stone];
            stone.place = Place::Leaving;
            stone.remove_when_landed = true;
            stone.fly(heap, 0.45, 1.0, 0.0, Event::Return);
        }
        let slot_in_lid = |stone: &Stone| match stone.place {
            Place::Lid { lid: whose, slot } if whose == lid => Some(slot),
            _ => None,
        };
        let mut in_lid: Vec<(usize, usize)> = self.stones.iter().enumerate().filter_map(|(index, stone)| Some((slot_in_lid(stone)?, index))).collect();
        in_lid.sort_by(|a, b| b.cmp(a));
        let mut delay = 0.08;
        for (order, point) in captured.iter().copied().enumerate() {
            // Should the lid hold fewer stones than were taken, the rest come out of it all the same.
            let stone = match in_lid.get(order) {
                Some((_, index)) => *index,
                None => {
                    let rest = self.lid_slot(lid, 0);
                    self.add(record.color.opposite(), Place::Lid { lid, slot: 0 }, rest.position, false, 0.0)
                }
            };
            let to = self.on_board(point);
            let stone = &mut self.stones[stone];
            stone.place = Place::Board(point);
            stone.fly(Rest::flat(to), 0.55, 1.3, delay, Event::Place);
            delay += 0.04;
        }
        self.lid_counts[lid] = self.lid_counts[lid].saturating_sub(captured.len());
    }

    /// The stones that a stone placed under the pointer would take glow and rise a little, about to go.
    pub fn preview(&mut self, taking: &[Point]) {
        for stone in &mut self.stones {
            let taken = !stone.flying && stone.on_board().is_some_and(|point| taking.contains(&point));
            stone.lift_target = if taken { 1.0 } else { 0.0 };
        }
    }

    pub fn update(&mut self, seconds: f32, events: &mut Vec<Event>) {
        for stone in &mut self.stones {
            stone.lift = approach(stone.lift, stone.lift_target, seconds / 0.15);
            if stone.dissolve_delay > 0.0 {
                stone.dissolve_delay -= seconds;
            } else {
                stone.dissolve = approach(stone.dissolve, stone.dissolve_target, seconds / FADE_TIME);
            }

            if !stone.flying {
                continue;
            }
            if stone.flight_delay > 0.0 {
                stone.flight_delay -= seconds;
                continue;
            }
            stone.flight_time = (stone.flight_time + seconds).min(stone.flight_duration);
            let t = stone.flight_time / stone.flight_duration;
            stone.position = stone.from.lerp(stone.to, ease(t)) + vec3(0.0, stone.arc * (PI * t).sin(), 0.0);
            if t >= 1.0 {
                stone.position = stone.to;
                stone.flying = false;
                events.push(stone.landing);
            }
        }
        self.stones.retain(|stone| !(stone.remove_when_gone && stone.dissolve >= 1.0) && !(stone.remove_when_landed && !stone.flying));
    }

    /// Nothing flying or dissolving: the position shown is the game's.
    pub fn settled(&self) -> bool {
        self.stones.iter().all(Stone::settled)
    }

    /// The stones of the board in their places, though stones taken may still be on their way to a lid.
    pub fn board_ready(&self) -> bool {
        self.stones.iter().filter(|stone| stone.on_board().is_some()).all(Stone::settled)
    }

    pub fn animating(&self) -> bool {
        // Stones about to be taken pulse while they are lifted.
        !self.settled() || self.stones.iter().any(|stone| stone.lift != stone.lift_target || stone.lift > 0.0)
    }

    /// Adds the stones to those to draw. `dead` stones are drawn seen through; stones about to be taken glow
    /// in `warning`.
    pub fn draws(&self, draws: &mut Vec<StoneDraw>, last_move: Option<Point>, dead: Option<&PointSet>, warning: Vec3, time: f32) {
        draws.reserve(self.stones.len());
        for stone in &self.stones {
            let orientation = if !stone.flying {
                Mat4::rotation(stone.tilt_axis, stone.tilt)
            } else if stone.flight_delay > 0.0 {
                Mat4::rotation(stone.from_axis, stone.from_tilt)
            } else {
                let e = ease(stone.flight_time / stone.flight_duration);
                Mat4::rotation(stone.tilt_axis, e * stone.tilt) * Mat4::rotation(stone.from_axis, (1.0 - e) * stone.from_tilt)
            };
            // The point of a stone at rest on the board.
            let resting = stone.on_board().filter(|_| !stone.flying);
            let lift = ease(stone.lift) * (0.8 + 0.2 * (time * 6.0).sin());
            draws.push(StoneDraw {
                model: Mat4::translation(stone.position + vec3(0.0, 0.12 * lift, 0.0)) * orientation * Mat4::rotation_y(stone.yaw),
                white: stone.color == Color::White,
                ground: if stone.flying { self.ground_at(stone.position) } else { stone.position.y - HALF_HEIGHT },
                glow_color: warning,
                glow: 1.5 * lift,
                dissolve: stone.dissolve,
                seed: stone.seed,
                mark: if resting.is_some() && resting == last_move { 1.0 } else { 0.0 },
                opacity: if resting.is_some_and(|point| dead.is_some_and(|dead| dead[point])) { 0.42 } else { 1.0 },
            });
        }
    }

    /// Marks the points with a stone lying on them, for the shade round the stones.
    pub fn mark_stones(&self, marks: &mut [u8; MAX_SIZE * MAX_SIZE]) {
        for stone in self.stones.iter().filter(|stone| !stone.flying && stone.dissolve < 0.5) {
            if let Some(point) = stone.on_board() {
                marks[go::x_of(point) + MAX_SIZE * go::y_of(point)] |= mark::STONE_HERE;
            }
        }
    }
}
