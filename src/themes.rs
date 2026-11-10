//! The looks of the set: the board, the stones, the bowls, the table and the studio light around them.

use std::sync::LazyLock;

use crate::math::{Vec2, Vec3, vec2, vec3};

#[derive(Clone)]
pub struct StoneLook {
    pub albedo: Vec3,
    pub roughness: f32,
    pub metallic: f32,
    pub clearcoat: f32,
    pub clearcoat_roughness: f32,
    pub subsurface: f32,
    pub pattern: f32, // 0 none, 1 shell, 2 slate, 3 glass, 4 yunzi
    pub pattern_strength: f32,
    pub trim: Vec3, // a glowing ring round the edge
    pub trim_strength: f32,
    pub edge: Vec3, // light coming through the thin edge
    pub edge_strength: f32,
}

impl Default for StoneLook {
    fn default() -> StoneLook {
        StoneLook {
            albedo: Vec3::ZERO,
            roughness: 0.3,
            metallic: 0.0,
            clearcoat: 0.0,
            clearcoat_roughness: 0.05,
            subsurface: 0.0,
            pattern: 0.0,
            pattern_strength: 0.0,
            trim: Vec3::ZERO,
            trim_strength: 0.0,
            edge: Vec3::ZERO,
            edge_strength: 0.0,
        }
    }
}

/// A lit panel of the studio, seen in the reflections.
#[derive(Clone)]
pub struct StudioPanel {
    pub direction: Vec3, // toward the panel
    pub roll: f32,
    pub half_size: Vec2, // as tangents of the angles
    pub corner: f32,
    pub softness: f32,
    pub radiance: Vec3,
    pub falloff: f32,
}

/// A panel that stands upright and fades evenly towards its edges.
fn panel(direction: Vec3, half_size: Vec2, corner: f32, softness: f32, radiance: Vec3) -> StudioPanel {
    StudioPanel { direction, roll: 0.0, half_size, corner, softness, radiance, falloff: 0.3 }
}

#[derive(Clone)]
pub struct Theme {
    pub name_english: &'static str,
    pub name_russian: &'static str,

    // The board.
    pub board_color: Vec3,
    pub board_roughness: f32,
    pub board_clearcoat: f32,
    pub board_clearcoat_roughness: f32,
    pub board_pattern: f32, // 0 kaya, 1 bamboo, 2 slate, 3 glass
    pub reflection: f32,
    pub reflection_blur: f32,
    pub grid_color: Vec3,
    pub grid_width: f32, // half the width of a line
    pub grid_metallic: f32,
    pub grid_roughness: f32,
    pub grid_glow: f32,
    pub star_radius: f32,

    // The bowls and their lids.
    pub bowl_color: Vec3,
    pub bowl_roughness: f32,
    pub bowl_clearcoat: f32,
    pub bowl_pattern: f32, // 0 wood, 1 lacquer
    pub bowl_seed: f32,

    pub white: StoneLook,
    pub black: StoneLook,

    pub table_color: Vec3,
    pub table_roughness: f32,
    pub table_pattern: f32, // 0 felt, 1 wood, 2 glowing grid, 3 stone
    pub table_sheen: f32,
    pub table_glow: f32,

    // The light.
    pub key_direction: Vec3,
    pub key_radiance: Vec3,
    pub key_softness: f32, // tangent of the light's angular radius
    pub ceiling: Vec3,
    pub horizon: Vec3,
    pub floor: Vec3,
    pub panels: Vec<StudioPanel>, // eight at most
    pub diffuse_light: f32,
    pub reflected_light: f32,
    pub background: f32,
    pub fog: Vec3,
    pub exposure: f32,
    pub bloom: f32,
    pub vignette: f32,

    pub accent: Vec3,      // highlights and the interface
    pub accent_warm: Vec3, // ko
}

impl Default for Theme {
    fn default() -> Theme {
        Theme {
            name_english: "",
            name_russian: "",
            board_color: Vec3::ZERO,
            board_roughness: 0.3,
            board_clearcoat: 0.0,
            board_clearcoat_roughness: 0.05,
            board_pattern: 0.0,
            reflection: 0.5,
            reflection_blur: 1.5,
            grid_color: Vec3::ZERO,
            grid_width: 0.02,
            grid_metallic: 0.0,
            grid_roughness: 0.5,
            grid_glow: 0.0,
            star_radius: 0.09,
            bowl_color: Vec3::ZERO,
            bowl_roughness: 0.35,
            bowl_clearcoat: 0.6,
            bowl_pattern: 0.0,
            bowl_seed: 0.3,
            white: StoneLook::default(),
            black: StoneLook::default(),
            table_color: Vec3::ZERO,
            table_roughness: 0.9,
            table_pattern: 0.0,
            table_sheen: 0.0,
            table_glow: 0.0,
            key_direction: Vec3::Y,
            key_radiance: Vec3::ZERO,
            key_softness: 0.04,
            ceiling: Vec3::ZERO,
            horizon: Vec3::ZERO,
            floor: Vec3::ZERO,
            panels: Vec::new(),
            diffuse_light: 1.0,
            reflected_light: 1.0,
            background: 0.6,
            fog: Vec3::ZERO,
            exposure: 1.0,
            bloom: 0.05,
            vignette: 0.35,
            accent: Vec3::ZERO,
            accent_warm: Vec3::ZERO,
        }
    }
}

pub const THEME_COUNT: usize = 4;

pub fn themes() -> &'static [Theme; THEME_COUNT] {
    static THEMES: LazyLock<[Theme; THEME_COUNT]> = LazyLock::new(|| [kaya(), bamboo(), glass(), neon()]);
    &THEMES
}

/// A kaya board with slate and shell stones, dark mulberry bowls, a dark wooden table.
fn kaya() -> Theme {
    Theme {
        name_english: "Kaya",
        name_russian: "Кая",
        board_color: vec3(0.64, 0.39, 0.13),
        board_roughness: 0.42,
        board_clearcoat: 0.3,
        board_clearcoat_roughness: 0.12,
        board_pattern: 0.0,
        reflection: 0.18,
        reflection_blur: 2.6,
        grid_color: vec3(0.012, 0.009, 0.006),
        grid_width: 0.021,
        grid_roughness: 0.55,
        star_radius: 0.095,
        bowl_color: vec3(0.15, 0.072, 0.03),
        bowl_roughness: 0.34,
        bowl_clearcoat: 0.75,
        bowl_pattern: 0.0,
        bowl_seed: 0.37,
        white: StoneLook {
            albedo: vec3(0.84, 0.83, 0.79),
            roughness: 0.26,
            clearcoat: 0.55,
            clearcoat_roughness: 0.07,
            subsurface: 0.22,
            pattern: 1.0,
            pattern_strength: 1.0,
            ..Default::default()
        },
        black: StoneLook {
            albedo: vec3(0.024, 0.024, 0.027),
            roughness: 0.44,
            clearcoat: 0.3,
            clearcoat_roughness: 0.22,
            pattern: 2.0,
            pattern_strength: 1.0,
            ..Default::default()
        },
        table_color: vec3(0.055, 0.032, 0.018),
        table_roughness: 0.5,
        table_pattern: 1.0,
        key_direction: vec3(-0.5, 0.78, 0.38),
        key_radiance: vec3(4.2, 3.9, 3.4),
        key_softness: 0.045,
        ceiling: vec3(0.035, 0.034, 0.034),
        horizon: vec3(0.1, 0.085, 0.07),
        floor: vec3(0.02, 0.016, 0.012),
        diffuse_light: 0.75,
        reflected_light: 1.0,
        background: 0.55,
        fog: vec3(0.032, 0.027, 0.022),
        exposure: 1.05,
        bloom: 0.035,
        vignette: 0.42,
        accent: vec3(0.3, 0.72, 1.0),
        accent_warm: vec3(1.0, 0.42, 0.18),
        panels: vec![
            panel(vec3(-0.5, 0.78, 0.38), vec2(0.42, 0.3), 0.08, 0.06, vec3(7.0, 6.4, 5.6)),
            panel(vec3(0.85, 0.3, 0.3), vec2(0.55, 0.35), 0.1, 0.15, vec3(0.7, 0.78, 0.9)),
            panel(vec3(0.15, 0.42, -0.9), vec2(1.3, 0.07), 0.03, 0.03, vec3(5.0, 4.6, 4.0)),
            panel(vec3(0.0, 1.0, 0.05), vec2(0.7, 0.7), 0.3, 0.3, vec3(0.6, 0.58, 0.55)),
            panel(vec3(-0.9, 0.25, -0.4), vec2(0.06, 0.6), 0.03, 0.03, vec3(2.6, 2.2, 1.8)),
        ],
        ..Default::default()
    }
}

/// A board of laminated bamboo with Chinese yunzi stones, red lacquer bowls and green felt.
fn bamboo() -> Theme {
    Theme {
        name_english: "Bamboo · Yunzi",
        name_russian: "Бамбук · юньцзы",
        board_color: vec3(0.6, 0.41, 0.19),
        board_roughness: 0.34,
        board_clearcoat: 0.5,
        board_clearcoat_roughness: 0.08,
        board_pattern: 1.0,
        reflection: 0.35,
        reflection_blur: 2.0,
        grid_color: vec3(0.022, 0.012, 0.006),
        grid_width: 0.019,
        grid_roughness: 0.45,
        star_radius: 0.09,
        bowl_color: vec3(0.3, 0.025, 0.018),
        bowl_roughness: 0.22,
        bowl_clearcoat: 0.9,
        bowl_pattern: 1.0,
        bowl_seed: 0.71,
        white: StoneLook {
            albedo: vec3(0.78, 0.77, 0.68),
            roughness: 0.48,
            clearcoat: 0.18,
            clearcoat_roughness: 0.18,
            subsurface: 0.4,
            pattern: 4.0,
            pattern_strength: 1.0,
            edge: vec3(0.55, 0.5, 0.36),
            edge_strength: 0.25,
            ..Default::default()
        },
        black: StoneLook {
            albedo: vec3(0.02, 0.025, 0.021),
            roughness: 0.46,
            clearcoat: 0.15,
            clearcoat_roughness: 0.2,
            pattern: 4.0,
            pattern_strength: 1.0,
            edge: vec3(0.04, 0.3, 0.09),
            edge_strength: 0.7,
            ..Default::default()
        },
        table_color: vec3(0.016, 0.045, 0.036),
        table_roughness: 0.95,
        table_pattern: 0.0,
        table_sheen: 0.35,
        key_direction: vec3(-0.58, 0.72, 0.4),
        key_radiance: vec3(4.4, 4.0, 3.4),
        key_softness: 0.05,
        ceiling: vec3(0.035, 0.033, 0.032),
        horizon: vec3(0.1, 0.085, 0.065),
        floor: vec3(0.018, 0.016, 0.012),
        diffuse_light: 0.7,
        reflected_light: 1.0,
        background: 0.55,
        fog: vec3(0.03, 0.028, 0.022),
        exposure: 1.05,
        bloom: 0.04,
        vignette: 0.42,
        accent: vec3(0.3, 0.92, 0.62),
        accent_warm: vec3(1.0, 0.45, 0.15),
        panels: vec![
            panel(vec3(-0.58, 0.72, 0.4), vec2(0.42, 0.3), 0.08, 0.06, vec3(7.4, 6.5, 5.4)),
            panel(vec3(0.85, 0.3, 0.3), vec2(0.55, 0.35), 0.1, 0.15, vec3(0.65, 0.8, 0.75)),
            panel(vec3(0.15, 0.42, -0.9), vec2(1.3, 0.07), 0.03, 0.03, vec3(5.0, 4.6, 4.0)),
            panel(vec3(0.0, 1.0, 0.05), vec2(0.7, 0.7), 0.3, 0.3, vec3(0.6, 0.58, 0.52)),
            panel(vec3(-0.9, 0.25, -0.4), vec2(0.06, 0.6), 0.03, 0.03, vec3(2.4, 2.1, 1.7)),
        ],
        ..Default::default()
    }
}

/// Smoked glass with silver lines, glass stones, black lacquer, a stone table.
fn glass() -> Theme {
    Theme {
        name_english: "Glass",
        name_russian: "Стекло",
        board_color: vec3(0.011, 0.013, 0.017),
        board_roughness: 0.07,
        board_clearcoat: 1.0,
        board_clearcoat_roughness: 0.03,
        board_pattern: 3.0,
        reflection: 1.0,
        reflection_blur: 0.7,
        grid_color: vec3(0.78, 0.8, 0.84),
        grid_width: 0.015,
        grid_metallic: 1.0,
        grid_roughness: 0.22,
        star_radius: 0.08,
        bowl_color: vec3(0.012, 0.012, 0.014),
        bowl_roughness: 0.1,
        bowl_clearcoat: 1.0,
        bowl_pattern: 1.0,
        bowl_seed: 0.2,
        white: StoneLook {
            albedo: vec3(0.84, 0.86, 0.9),
            roughness: 0.2,
            clearcoat: 0.9,
            clearcoat_roughness: 0.03,
            subsurface: 0.5,
            pattern: 3.0,
            pattern_strength: 0.5,
            edge: vec3(0.55, 0.66, 0.85),
            edge_strength: 0.35,
            ..Default::default()
        },
        black: StoneLook {
            albedo: vec3(0.008, 0.009, 0.013),
            roughness: 0.06,
            clearcoat: 1.0,
            clearcoat_roughness: 0.02,
            pattern: 3.0,
            pattern_strength: 0.6,
            edge: vec3(0.06, 0.12, 0.35),
            edge_strength: 0.6,
            ..Default::default()
        },
        table_color: vec3(0.024, 0.024, 0.028),
        table_roughness: 0.3,
        table_pattern: 3.0,
        key_direction: vec3(-0.42, 0.85, 0.32),
        key_radiance: vec3(3.1, 3.15, 3.3),
        key_softness: 0.05,
        ceiling: vec3(0.05, 0.055, 0.065),
        horizon: vec3(0.09, 0.095, 0.11),
        floor: vec3(0.018, 0.018, 0.02),
        diffuse_light: 1.0,
        reflected_light: 1.0,
        background: 0.5,
        fog: vec3(0.03, 0.032, 0.038),
        exposure: 1.0,
        bloom: 0.04,
        vignette: 0.4,
        accent: vec3(0.4, 0.78, 1.0),
        accent_warm: vec3(0.95, 0.6, 0.3),
        panels: vec![
            panel(vec3(-0.42, 0.85, 0.32), vec2(0.5, 0.5), 0.25, 0.12, vec3(6.0, 6.1, 6.4)),
            panel(vec3(0.9, 0.2, 0.25), vec2(0.5, 0.3), 0.1, 0.12, vec3(0.8, 0.85, 1.0)),
            panel(vec3(-0.1, 0.35, -0.92), vec2(1.4, 0.05), 0.02, 0.02, vec3(6.0, 6.2, 6.6)),
            panel(vec3(0.0, 1.0, 0.0), vec2(0.8, 0.8), 0.4, 0.35, vec3(0.9, 0.92, 1.0)),
            panel(vec3(0.6, 0.3, -0.75), vec2(0.05, 0.7), 0.02, 0.02, vec3(2.5, 2.6, 3.0)),
        ],
        ..Default::default()
    }
}

/// A dark board with glowing lines and stones with neon rims.
fn neon() -> Theme {
    Theme {
        name_english: "Neon",
        name_russian: "Неон",
        board_color: vec3(0.006, 0.008, 0.014),
        board_roughness: 0.12,
        board_clearcoat: 0.6,
        board_clearcoat_roughness: 0.03,
        board_pattern: 3.0,
        reflection: 0.9,
        reflection_blur: 0.6,
        grid_color: vec3(0.2, 0.8, 1.0),
        grid_width: 0.013,
        grid_roughness: 0.3,
        grid_glow: 1.6,
        star_radius: 0.075,
        bowl_color: vec3(0.05, 0.055, 0.07),
        bowl_roughness: 0.25,
        bowl_clearcoat: 0.8,
        bowl_pattern: 1.0,
        bowl_seed: 0.5,
        white: StoneLook {
            albedo: vec3(0.82, 0.86, 0.9),
            roughness: 0.2,
            clearcoat: 1.0,
            clearcoat_roughness: 0.03,
            subsurface: 0.5,
            trim: vec3(1.0, 0.3, 0.75),
            trim_strength: 5.0,
            ..Default::default()
        },
        black: StoneLook {
            albedo: vec3(0.03, 0.032, 0.04),
            roughness: 0.18,
            clearcoat: 1.0,
            clearcoat_roughness: 0.03,
            pattern: 3.0,
            pattern_strength: 0.4,
            trim: vec3(0.2, 0.8, 1.0),
            trim_strength: 5.0,
            ..Default::default()
        },
        table_color: vec3(0.004, 0.006, 0.01),
        table_roughness: 0.5,
        table_pattern: 2.0,
        table_glow: 0.5,
        key_direction: vec3(-0.35, 0.8, 0.48),
        key_radiance: vec3(1.9, 2.2, 2.7),
        key_softness: 0.03,
        ceiling: vec3(0.01, 0.012, 0.025),
        horizon: vec3(0.02, 0.035, 0.07),
        floor: vec3(0.004, 0.005, 0.008),
        diffuse_light: 1.0,
        reflected_light: 1.2,
        background: 0.7,
        fog: vec3(0.006, 0.01, 0.02),
        exposure: 1.1,
        bloom: 0.07,
        vignette: 0.5,
        accent: vec3(0.2, 0.85, 1.0),
        accent_warm: vec3(1.0, 0.35, 0.75),
        panels: vec![
            panel(vec3(-0.35, 0.8, 0.48), vec2(0.35, 0.25), 0.05, 0.05, vec3(3.5, 4.2, 5.0)),
            panel(vec3(0.95, 0.12, 0.2), vec2(1.0, 0.03), 0.02, 0.02, vec3(8.0, 1.5, 5.0)),
            panel(vec3(-0.95, 0.12, -0.2), vec2(1.0, 0.03), 0.02, 0.02, vec3(1.0, 5.0, 8.0)),
            panel(vec3(0.1, 0.3, -0.95), vec2(1.4, 0.025), 0.01, 0.015, vec3(8.0, 4.0, 1.0)),
            panel(vec3(0.0, 1.0, 0.0), vec2(0.6, 0.6), 0.3, 0.3, vec3(0.25, 0.3, 0.45)),
            panel(vec3(0.3, 0.25, 0.95), vec2(0.9, 0.025), 0.01, 0.015, vec3(1.0, 6.0, 7.0)),
        ],
        ..Default::default()
    }
}
