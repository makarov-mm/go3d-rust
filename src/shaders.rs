//! The GLSL sources, built into the executable.

pub const COMMON: &str = include_str!("../shaders/common.glsl");
pub const LIGHTING: &str = include_str!("../shaders/lighting.glsl");
pub const IBL_COMMON: &str = include_str!("../shaders/ibl_common.glsl");

pub const STONE_VERT: &str = include_str!("../shaders/stone.vert");
pub const STONE_FRAG: &str = include_str!("../shaders/stone.frag");
pub const SURFACE_VERT: &str = include_str!("../shaders/surface.vert");
pub const SURFACE_FRAG: &str = include_str!("../shaders/surface.frag");
pub const SHADOW_VERT: &str = include_str!("../shaders/shadow.vert");
pub const SHADOW_FRAG: &str = include_str!("../shaders/shadow.frag");

pub const FULLSCREEN_VERT: &str = include_str!("../shaders/fullscreen.vert");
pub const BACKGROUND_FRAG: &str = include_str!("../shaders/background.frag");
pub const RESOLVE_FRAG: &str = include_str!("../shaders/resolve.frag");
pub const BLOOM_DOWN_FRAG: &str = include_str!("../shaders/bloom_down.frag");
pub const BLOOM_UP_FRAG: &str = include_str!("../shaders/bloom_up.frag");
pub const COMPOSITE_FRAG: &str = include_str!("../shaders/composite.frag");

pub const ENVIRONMENT_COMP: &str = include_str!("../shaders/environment.comp");
pub const IRRADIANCE_COMP: &str = include_str!("../shaders/irradiance.comp");
pub const PREFILTER_COMP: &str = include_str!("../shaders/prefilter.comp");
pub const BRDF_COMP: &str = include_str!("../shaders/brdf.comp");

pub const UI_VERT: &str = include_str!("../shaders/ui.vert");
pub const UI_FRAG: &str = include_str!("../shaders/ui.frag");
