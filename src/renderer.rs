//! Draws the scene into the window: shadows, reflections, the lit scene in high dynamic range with
//! multisampling, then bloom and tone mapping. The interface is drawn on top afterwards.

use std::mem::{offset_of, size_of};

use crate::Result;
use crate::board_geometry::{self as board, Layout};
use crate::gfx::{self, Attribute, Buffer, Framebuffer, Program, Sampler, Stage, Texture, VertexArray};
use crate::go::MAX_SIZE;
use crate::math::{Mat4, Vec3, Vec4, vec2, vec3, vec4};
use crate::shaders;
use crate::stone_model::{self as stones, StoneShape};
use crate::text::{self, FontLibrary};
use crate::themes::Theme;

/// A stone, in the look of its color.
#[derive(Clone, Copy)]
pub struct StoneDraw {
    pub model: Mat4,
    pub white: bool,
    pub ground: f32, // the height of what it lies on
    pub glow_color: Vec3,
    pub glow: f32,
    pub dissolve: f32,
    pub seed: f32,    // where the pattern falls
    pub mark: f32,    // the ring on the stone played last
    pub opacity: f32, // below 1, seen through: a dead stone, or where a click would play
}

/// What is marked on each point of the board, as bits; see surface.frag.
pub mod mark {
    pub const BLACK_AREA: u8 = 1;
    pub const WHITE_AREA: u8 = 2;
    pub const HOVER: u8 = 4;
    pub const HINT: u8 = 8;
    pub const KO: u8 = 16;
    pub const STONE_HERE: u8 = 32;
}

/// A rectangle of the screen behind which the scene is blurred, in pixels from the top left.
#[derive(Clone, Copy)]
pub struct FrostedPanel {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
    pub opacity: f32,
}

pub struct Scene {
    pub view: Mat4,
    pub projection: Mat4,
    pub eye: Vec3,
    pub time: f32,
    pub stones: Vec<StoneDraw>,
    pub marks: [u8; MAX_SIZE * MAX_SIZE], // column + MAX_SIZE * row, rows from the far side
    pub estimate: bool,                   // the territory marked as an estimate, fainter
    pub coordinates: bool,
    pub panels: Vec<FrostedPanel>,
}

const SHADOW_SIZE: i32 = 2048;
const ENVIRONMENT_SIZE: i32 = 512;
const IRRADIANCE_SIZE: i32 = 32;
const PREFILTERED_SIZE: i32 = 256;
const PREFILTERED_LEVELS: i32 = 6;
const BRDF_SIZE: i32 = 128;
const LABEL_SIZE: i32 = 4096;
const TABLE_RADIUS: f32 = 90.0;
const MAX_PANELS: usize = 8; // the lit panels of the studio, in environment.comp
const MAX_FROSTED: usize = 4; // the panels of the interface, in composite.frag
const BLOOM_LEVELS: usize = 6;

/// The std140 layout of the per-frame uniform block in common.glsl.
#[repr(C)]
#[derive(Clone, Copy)]
struct FrameBlock {
    view_projection: Mat4,
    inverse_view_projection: Mat4,
    light_view_projection: Mat4,
    camera_position: Vec4,
    light_direction: Vec4,
    light_color: Vec4,
    shadow_params: Vec4,
    viewport: Vec4,
    environment: Vec4,
    accent: Vec4,
    accent_warm: Vec4,
    fog: Vec4,
}

/// The std140 layout of the per-pass uniform block.
#[repr(C)]
#[derive(Clone, Copy)]
struct PassBlock {
    eye: Vec4,
    mirror: Vec4,
}

/// A stone in the std430 buffer of stone.vert.
#[repr(C)]
#[derive(Clone, Copy)]
struct StoneInstance {
    model: Mat4,
    glow: Vec4,   // rgb, strength
    params: Vec4, // dissolve, seed, height of the ground, mark of the last move
    extra: Vec4,  // opacity
}

impl StoneInstance {
    fn of(stone: &StoneDraw) -> StoneInstance {
        StoneInstance {
            model: stone.model,
            glow: stone.glow_color.extend(stone.glow),
            params: vec4(stone.dissolve, stone.seed, stone.ground, stone.mark),
            extra: vec4(stone.opacity, 0.0, 0.0, 0.0),
        }
    }
}

/// A run of stones in the instance buffer drawn with one call.
#[derive(Clone, Copy)]
struct StoneRange {
    first: usize,
    count: usize,
    white: bool,
}

fn levels_for(size: i32) -> i32 {
    (size.max(1).ilog2() + 1) as i32
}

#[derive(Default)]
struct GpuMesh {
    vertex_array: VertexArray,
    _vertices: Buffer,
    _indices: Buffer,
    count: i32,
}

impl GpuMesh {
    fn new<V: Copy>(vertices: &[V], indices: &[u32], attributes: &[Attribute]) -> GpuMesh {
        let vertex_buffer = Buffer::with_data(gfx::bytes_of(vertices));
        let index_buffer = Buffer::with_data(gfx::bytes_of(indices));
        let vertex_array = VertexArray::new(&vertex_buffer, size_of::<V>(), attributes, Some(&index_buffer));
        GpuMesh { vertex_array, _vertices: vertex_buffer, _indices: index_buffer, count: indices.len() as i32 }
    }

    fn of_stone(mesh: &stones::Mesh) -> GpuMesh {
        let attributes = [
            Attribute { size: 3, kind: gl::FLOAT, offset: offset_of!(stones::Vertex, position) },
            Attribute { size: 3, kind: gl::FLOAT, offset: offset_of!(stones::Vertex, normal) },
        ];
        GpuMesh::new(&mesh.vertices, &mesh.indices, &attributes)
    }

    fn of_board(mesh: &board::Mesh) -> GpuMesh {
        let attributes = [
            Attribute { size: 3, kind: gl::FLOAT, offset: offset_of!(board::Vertex, position) },
            Attribute { size: 3, kind: gl::FLOAT, offset: offset_of!(board::Vertex, normal) },
            Attribute { size: 2, kind: gl::FLOAT, offset: offset_of!(board::Vertex, uv) },
        ];
        GpuMesh::new(&mesh.vertices, &mesh.indices, &attributes)
    }

    fn draw(&self) {
        self.vertex_array.bind();
        gfx::draw_indexed_triangles(self.count);
    }

    fn draw_instances(&self, instances: usize) {
        if instances > 0 {
            self.vertex_array.bind();
            gfx::draw_indexed_triangles_instanced(self.count, instances as i32);
        }
    }
}

/// A level of the bloom: the image at half the size of the level before.
#[derive(Default)]
struct BloomLevel {
    texture: Texture,
    framebuffer: Framebuffer,
    width: i32,
    height: i32,
}

pub struct Renderer {
    width: i32,
    height: i32,
    samples: i32,
    theme: Theme,
    layout: Layout,
    light_view_projection: Mat4,
    shadow_radius: f32,
    shadow_range: f32,

    stone: GpuMesh,
    simple_stone: GpuMesh,
    board_top: GpuMesh,
    board_sides: GpuMesh,
    table: GpuMesh,
    bowl: GpuMesh,
    lid: GpuMesh,
    empty_vertex_array: VertexArray,

    stone_program: Program,
    surface_program: Program,
    shadow_program: Program,
    background_program: Program,
    resolve_program: Program,
    bloom_down_program: Program,
    bloom_up_program: Program,
    composite_program: Program,
    environment_program: Program,
    irradiance_program: Program,
    prefilter_program: Program,

    frame_data: Buffer,
    pass_data: Buffer,
    stone_buffer: Buffer,
    opaque: [StoneRange; 2],      // Black's stones, then White's
    translucent: Vec<StoneRange>, // one at a time, the furthest first

    shadow_map: Texture,
    shadow_framebuffer: Framebuffer,
    shadow_compare_sampler: Sampler,
    shadow_depth_sampler: Sampler,
    environment: Texture,
    irradiance: Texture,
    prefiltered: Texture,
    brdf: Texture,
    labels: Texture,
    marks: Texture,

    scene_color: Texture,
    scene_depth: Texture,
    resolved: Texture,
    scene_framebuffer: Framebuffer,
    resolved_framebuffer: Framebuffer,
    reflection_color: Texture,
    reflection_depth: Texture,
    reflection_framebuffer: Framebuffer,
    bloom: [BloomLevel; BLOOM_LEVELS],
}

impl Renderer {
    pub fn new(stone: &StoneShape, fonts: &mut FontLibrary, theme: &Theme, layout: &Layout) -> Result<Renderer> {
        let scene = |name, vertex, fragment| {
            Program::new(
                name,
                &[
                    Stage { kind: gl::VERTEX_SHADER, sources: &[shaders::COMMON, vertex] },
                    Stage { kind: gl::FRAGMENT_SHADER, sources: &[shaders::COMMON, shaders::LIGHTING, fragment] },
                ],
            )
        };
        let screen = |name, fragment| {
            Program::new(
                name,
                &[
                    Stage { kind: gl::VERTEX_SHADER, sources: &[shaders::FULLSCREEN_VERT] },
                    Stage { kind: gl::FRAGMENT_SHADER, sources: &[shaders::COMMON, fragment] },
                ],
            )
        };
        let compute = |name, source| Program::new(name, &[Stage { kind: gl::COMPUTE_SHADER, sources: &[shaders::IBL_COMMON, source] }]);

        let stone_program = scene("stone", shaders::STONE_VERT, shaders::STONE_FRAG)?;
        let surface_program = scene("surface", shaders::SURFACE_VERT, shaders::SURFACE_FRAG)?;
        let shadow_program = Program::new(
            "shadow",
            &[
                Stage { kind: gl::VERTEX_SHADER, sources: &[shaders::COMMON, shaders::SHADOW_VERT] },
                Stage { kind: gl::FRAGMENT_SHADER, sources: &[shaders::COMMON, shaders::SHADOW_FRAG] },
            ],
        )?;
        let background_program = screen("background", shaders::BACKGROUND_FRAG)?;
        let resolve_program = screen("resolve", shaders::RESOLVE_FRAG)?;
        let bloom_down_program = screen("bloom", shaders::BLOOM_DOWN_FRAG)?;
        let bloom_up_program = screen("bloom", shaders::BLOOM_UP_FRAG)?;
        let composite_program = screen("composite", shaders::COMPOSITE_FRAG)?;
        let environment_program = compute("environment", shaders::ENVIRONMENT_COMP)?;
        let irradiance_program = compute("irradiance", shaders::IRRADIANCE_COMP)?;
        let prefilter_program = compute("prefilter", shaders::PREFILTER_COMP)?;
        let brdf_program = compute("brdf", shaders::BRDF_COMP)?;

        let frame_data = Buffer::with_size(size_of::<FrameBlock>());
        let pass_data = Buffer::with_size(size_of::<PassBlock>());
        frame_data.bind_uniform_block(0);
        pass_data.bind_uniform_block(1);

        // The shadow map, read both with comparison (for filtering) and without (to find the casters).
        let shadow_map = Texture::new(gl::TEXTURE_2D);
        shadow_map.storage(1, gl::DEPTH_COMPONENT32F, SHADOW_SIZE, SHADOW_SIZE);
        let shadow_framebuffer = Framebuffer::new();
        shadow_framebuffer.attach(gl::DEPTH_ATTACHMENT, &shadow_map);
        shadow_framebuffer.without_color();
        shadow_framebuffer.check("shadow")?;
        let (shadow_compare_sampler, shadow_depth_sampler) = (Sampler::new(), Sampler::new());
        for sampler in [&shadow_compare_sampler, &shadow_depth_sampler] {
            sampler.parameter(gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
            sampler.parameter(gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
        }
        shadow_compare_sampler.parameter(gl::TEXTURE_MIN_FILTER, gl::LINEAR);
        shadow_compare_sampler.parameter(gl::TEXTURE_MAG_FILTER, gl::LINEAR);
        shadow_compare_sampler.parameter(gl::TEXTURE_COMPARE_MODE, gl::COMPARE_REF_TO_TEXTURE);
        shadow_compare_sampler.parameter(gl::TEXTURE_COMPARE_FUNC, gl::LEQUAL);
        shadow_depth_sampler.parameter(gl::TEXTURE_MIN_FILTER, gl::NEAREST);
        shadow_depth_sampler.parameter(gl::TEXTURE_MAG_FILTER, gl::NEAREST);

        // The light probes, baked when the theme is set, and the table of the reflectance, baked here once.
        gfx::enable(gl::TEXTURE_CUBE_MAP_SEAMLESS);
        let cube = |size, levels| {
            let texture = Texture::new(gl::TEXTURE_CUBE_MAP);
            texture.storage(levels, gl::RGBA16F, size, size);
            texture.set_filter(if levels > 1 { gl::LINEAR_MIPMAP_LINEAR } else { gl::LINEAR }, gl::LINEAR, gl::CLAMP_TO_EDGE);
            texture
        };
        let environment = cube(ENVIRONMENT_SIZE, levels_for(ENVIRONMENT_SIZE));
        let irradiance = cube(IRRADIANCE_SIZE, 1);
        let prefiltered = cube(PREFILTERED_SIZE, PREFILTERED_LEVELS);
        let brdf = Texture::new(gl::TEXTURE_2D);
        brdf.storage(1, gl::RG16F, BRDF_SIZE, BRDF_SIZE);
        brdf.set_filter(gl::LINEAR, gl::LINEAR, gl::CLAMP_TO_EDGE);
        brdf_program.bind();
        brdf.bind_image(0, 0, false, gl::RG16F);
        gfx::dispatch_compute(BRDF_SIZE, BRDF_SIZE, 1);

        // What is marked on each point, read by the shader point by point.
        let marks = Texture::new(gl::TEXTURE_2D);
        marks.storage(1, gl::R8UI, MAX_SIZE as i32, MAX_SIZE as i32);
        marks.set_filter(gl::NEAREST, gl::NEAREST, gl::CLAMP_TO_EDGE);

        let samples =
            [gl::MAX_SAMPLES, gl::MAX_COLOR_TEXTURE_SAMPLES, gl::MAX_DEPTH_TEXTURE_SAMPLES].into_iter().map(gfx::get_integer).fold(4, i32::min).max(1);

        let mut renderer = Renderer {
            width: 0,
            height: 0,
            samples,
            theme: theme.clone(),
            layout: layout.clone(),
            light_view_projection: Mat4::IDENTITY,
            shadow_radius: 10.0,
            shadow_range: 30.0,
            stone: GpuMesh::of_stone(&stone.detailed),
            simple_stone: GpuMesh::of_stone(&stone.simple),
            board_top: GpuMesh::default(),
            board_sides: GpuMesh::default(),
            table: GpuMesh::default(),
            bowl: GpuMesh::default(),
            lid: GpuMesh::default(),
            empty_vertex_array: VertexArray::empty(),
            stone_program,
            surface_program,
            shadow_program,
            background_program,
            resolve_program,
            bloom_down_program,
            bloom_up_program,
            composite_program,
            environment_program,
            irradiance_program,
            prefilter_program,
            frame_data,
            pass_data,
            stone_buffer: Buffer::streaming(),
            opaque: [StoneRange { first: 0, count: 0, white: false }; 2],
            translucent: Vec::new(),
            shadow_map,
            shadow_framebuffer,
            shadow_compare_sampler,
            shadow_depth_sampler,
            environment,
            irradiance,
            prefiltered,
            brdf,
            labels: Texture::default(),
            marks,
            scene_color: Texture::default(),
            scene_depth: Texture::default(),
            resolved: Texture::default(),
            scene_framebuffer: Framebuffer::default(),
            resolved_framebuffer: Framebuffer::default(),
            reflection_color: Texture::default(),
            reflection_depth: Texture::default(),
            reflection_framebuffer: Framebuffer::default(),
            bloom: Default::default(),
        };
        renderer.set_theme(theme);
        renderer.set_layout(layout, fonts);
        Ok(renderer)
    }

    pub fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
        self.update_light_view();
        self.bake_environment();
    }

    /// The board, the bowls and the lids for a size of board.
    pub fn set_layout(&mut self, layout: &Layout, fonts: &mut FontLibrary) {
        self.layout = layout.clone();
        self.board_top = GpuMesh::of_board(&board::build_board_top(layout));
        self.board_sides = GpuMesh::of_board(&board::build_board_sides(layout));
        self.table = GpuMesh::of_board(&board::build_table(TABLE_RADIUS, layout.table_height()));
        self.bowl = GpuMesh::of_board(&board::build_bowl(layout.bowl_radius, layout.bowl_height));
        self.lid = GpuMesh::of_board(&board::build_lid(layout.bowl_radius));

        // The coordinates written round the lines.
        self.labels = Texture::new(gl::TEXTURE_2D);
        self.labels.storage(levels_for(LABEL_SIZE), gl::R8, LABEL_SIZE, LABEL_SIZE);
        self.labels.upload_r8(LABEL_SIZE, LABEL_SIZE, &text::render_board_labels(fonts, LABEL_SIZE, layout.size, layout.half, layout.board_half));
        self.labels.generate_mipmap();
        self.labels.set_filter(gl::LINEAR_MIPMAP_LINEAR, gl::LINEAR, gl::CLAMP_TO_EDGE);
        self.labels.parameter_f32(gfx::TEXTURE_MAX_ANISOTROPY, gfx::max_anisotropy().min(8.0));

        self.update_light_view();
    }

    /// The key light's view: a box round everything that casts or takes a shadow.
    fn update_light_view(&mut self) {
        let layout = &self.layout;
        let reach = |place: &Vec3| vec3(place.x, 0.0, place.z).length() + layout.bowl_radius;
        let radius = layout.bowls.iter().chain(&layout.lids).map(reach).fold(layout.board_half * 1.42, f32::max);
        self.shadow_radius = radius + 0.5;
        self.shadow_range = 2.0 * self.shadow_radius + 16.0;

        let to_light = self.theme.key_direction.normalized();
        let up = if to_light.y.abs() > 0.99 { vec3(0.0, 0.0, 1.0) } else { Vec3::Y };
        let view = Mat4::look_at(to_light * (self.shadow_radius + 8.0), Vec3::ZERO, up);
        let projection = Mat4::orthographic(-self.shadow_radius, self.shadow_radius, -self.shadow_radius, self.shadow_radius, 0.5, 0.5 + self.shadow_range);
        self.light_view_projection = projection * view;
    }

    /// The studio light of the theme, baked into the light probes.
    fn bake_environment(&self) {
        let theme = &self.theme;
        let environment = &self.environment_program;
        environment.set("uCeiling", theme.ceiling);
        environment.set("uHorizon", theme.horizon);
        environment.set("uFloor", theme.floor);
        let (mut directions, mut shapes, mut colors) = ([Vec4::default(); MAX_PANELS], [Vec4::default(); MAX_PANELS], [Vec4::default(); MAX_PANELS]);
        let count = theme.panels.len().min(MAX_PANELS);
        for (i, panel) in theme.panels.iter().take(count).enumerate() {
            directions[i] = panel.direction.extend(panel.roll);
            shapes[i] = vec4(panel.half_size.x, panel.half_size.y, panel.corner, panel.softness);
            colors[i] = panel.radiance.extend(panel.falloff);
        }
        environment.set("uPanelDirection", &directions[..]);
        environment.set("uPanelShape", &shapes[..]);
        environment.set("uPanelColor", &colors[..]);
        environment.set("uPanelCount", count as i32);
        environment.bind();
        self.environment.bind_image(0, 0, true, gl::RGBA16F);
        gfx::dispatch_compute(ENVIRONMENT_SIZE, ENVIRONMENT_SIZE, 6);
        self.environment.generate_mipmap();

        self.environment.bind(0);
        self.irradiance_program.set("uSourceLod", 4.0f32);
        self.irradiance_program.bind();
        self.irradiance.bind_image(0, 0, true, gl::RGBA16F);
        gfx::dispatch_compute(IRRADIANCE_SIZE, IRRADIANCE_SIZE, 6);

        self.prefilter_program.set("uSourceSize", ENVIRONMENT_SIZE as f32);
        self.prefilter_program.bind();
        for level in 0..PREFILTERED_LEVELS {
            let size = PREFILTERED_SIZE >> level;
            self.prefilter_program.set("uRoughness", level as f32 / (PREFILTERED_LEVELS - 1) as f32);
            self.prefiltered.bind_image(0, level, true, gl::RGBA16F);
            gfx::dispatch_compute(size, size, 6);
        }
    }

    pub fn resize(&mut self, width: i32, height: i32) -> Result<()> {
        let (width, height) = (width.max(1), height.max(1));
        if width == self.width && height == self.height {
            return Ok(());
        }
        self.width = width;
        self.height = height;
        self.create_screen_targets()
    }

    fn create_screen_targets(&mut self) -> Result<()> {
        let (width, height) = (self.width, self.height);
        self.scene_color = Texture::new(gl::TEXTURE_2D_MULTISAMPLE);
        self.scene_color.storage_multisample(self.samples, gl::RGBA16F, width, height);
        self.scene_depth = Texture::new(gl::TEXTURE_2D_MULTISAMPLE);
        self.scene_depth.storage_multisample(self.samples, gl::DEPTH_COMPONENT32F, width, height);
        self.scene_framebuffer = Framebuffer::new();
        self.scene_framebuffer.attach(gl::COLOR_ATTACHMENT0, &self.scene_color);
        self.scene_framebuffer.attach(gl::DEPTH_ATTACHMENT, &self.scene_depth);
        self.scene_framebuffer.check("scene")?;

        self.resolved = Texture::new(gl::TEXTURE_2D);
        self.resolved.storage(1, gl::RGBA16F, width, height);
        self.resolved.set_filter(gl::LINEAR, gl::LINEAR, gl::CLAMP_TO_EDGE);
        self.resolved_framebuffer = Framebuffer::new();
        self.resolved_framebuffer.attach(gl::COLOR_ATTACHMENT0, &self.resolved);
        self.resolved_framebuffer.check("resolve")?;

        // The reflections at half size, blurred through their mip levels.
        let (reflection_width, reflection_height) = ((width / 2).max(1), (height / 2).max(1));
        let reflection_levels = levels_for(reflection_width.max(reflection_height)).min(5);
        self.reflection_color = Texture::new(gl::TEXTURE_2D);
        self.reflection_color.storage(reflection_levels, gl::RGBA16F, reflection_width, reflection_height);
        self.reflection_color.set_filter(gl::LINEAR_MIPMAP_LINEAR, gl::LINEAR, gl::CLAMP_TO_EDGE);
        self.reflection_depth = Texture::new(gl::TEXTURE_2D);
        self.reflection_depth.storage(1, gl::DEPTH_COMPONENT24, reflection_width, reflection_height);
        self.reflection_framebuffer = Framebuffer::new();
        self.reflection_framebuffer.attach(gl::COLOR_ATTACHMENT0, &self.reflection_color);
        self.reflection_framebuffer.attach(gl::DEPTH_ATTACHMENT, &self.reflection_depth);
        self.reflection_framebuffer.check("reflection")?;

        for (level, bloom) in self.bloom.iter_mut().enumerate() {
            bloom.width = (width >> (level + 1)).max(1);
            bloom.height = (height >> (level + 1)).max(1);
            bloom.texture = Texture::new(gl::TEXTURE_2D);
            bloom.texture.storage(1, gl::R11F_G11F_B10F, bloom.width, bloom.height);
            bloom.texture.set_filter(gl::LINEAR, gl::LINEAR, gl::CLAMP_TO_EDGE);
            bloom.framebuffer = Framebuffer::new();
            bloom.framebuffer.attach(gl::COLOR_ATTACHMENT0, &bloom.texture);
            bloom.framebuffer.check("bloom")?;
        }
        Ok(())
    }

    pub fn render(&mut self, scene: &Scene) {
        self.update_frame_data(scene);
        self.upload_stones(scene);
        self.marks.upload_r8ui(MAX_SIZE as i32, MAX_SIZE as i32, &scene.marks);
        self.draw_shadow_map();
        self.draw_reflections(scene);
        self.draw_main_pass(scene);
        self.post_process(scene);
    }

    fn update_frame_data(&self, scene: &Scene) {
        let theme = &self.theme;
        let view_projection = scene.projection * scene.view;
        let (width, height) = (self.width as f32, self.height as f32);
        // The depth bias, the same in world units for every size of board; the fog begins beyond the board.
        let fog_start = scene.eye.length() + self.layout.extent() * 0.8;
        let block = FrameBlock {
            view_projection,
            inverse_view_projection: view_projection.inverse(),
            light_view_projection: self.light_view_projection,
            camera_position: scene.eye.extend(scene.time),
            light_direction: theme.key_direction.normalized().extend(2.0 * self.shadow_radius / SHADOW_SIZE as f32),
            light_color: theme.key_radiance.extend(theme.key_softness),
            shadow_params: vec4(self.shadow_range, 0.012 / self.shadow_range, fog_start, 0.0),
            viewport: vec4(width, height, 1.0 / width, 1.0 / height),
            environment: vec4(theme.diffuse_light, theme.reflected_light, (PREFILTERED_LEVELS - 1) as f32, theme.background),
            accent: theme.accent.extend(1.0),
            accent_warm: theme.accent_warm.extend(1.0),
            fog: theme.fog.extend(0.05),
        };
        self.frame_data.write(gfx::bytes_of(&[block]));
    }

    /// The stones into the instance buffer: Black's opaque stones, White's, then those seen through, the
    /// furthest first, to be drawn one at a time.
    fn upload_stones(&mut self, scene: &Scene) {
        let mut instances = Vec::with_capacity(scene.stones.len());
        for (range, white) in self.opaque.iter_mut().zip([false, true]) {
            let first = instances.len();
            instances.extend(scene.stones.iter().filter(|stone| stone.white == white && stone.opacity >= 1.0).map(StoneInstance::of));
            *range = StoneRange { first, count: instances.len() - first, white };
        }
        let distance = |stone: &StoneDraw| (stone.model.transform_point(Vec3::ZERO) - scene.eye).length();
        let mut see_through: Vec<&StoneDraw> = scene.stones.iter().filter(|stone| stone.opacity < 1.0).collect();
        see_through.sort_by(|a, b| distance(b).total_cmp(&distance(a)));
        self.translucent.clear();
        for stone in see_through {
            self.translucent.push(StoneRange { first: instances.len(), count: 1, white: stone.white });
            instances.push(StoneInstance::of(stone));
        }

        if !instances.is_empty() {
            self.stone_buffer.stream(gfx::bytes_of(&instances));
        }
        self.stone_buffer.bind_storage_block(0);
    }

    fn set_pass(&self, eye: Vec3, shadows: bool, mirror: bool) {
        let block =
            PassBlock { eye: eye.extend(if shadows { 0.0 } else { 1.0 }), mirror: vec4(0.0, if mirror { 1.0 } else { 0.0 }, self.layout.board_half, 0.0) };
        self.pass_data.write(gfx::bytes_of(&[block]));
    }

    fn bind_lighting(&self, reflections: bool) {
        self.shadow_map.bind(0);
        self.shadow_compare_sampler.bind(0);
        self.shadow_map.bind(1);
        self.shadow_depth_sampler.bind(1);
        self.irradiance.bind(2);
        self.prefiltered.bind(3);
        self.brdf.bind(4);
        if reflections {
            self.reflection_color.bind(5);
        } else {
            gfx::unbind_texture(5);
        }
        self.labels.bind(6);
        self.marks.bind(7);
    }

    /// The bowls and the lids, each turned its own way so that the grain differs.
    fn draw_bowls(&self, program: &Program, material: bool) {
        let layout = &self.layout;
        for (i, (bowl, lid)) in layout.bowls.into_iter().zip(layout.lids).enumerate() {
            program.set("uModel", Mat4::translation(bowl) * Mat4::rotation_y(1.3 + 2.1 * i as f32));
            if material {
                program.set("uBowlShape", vec4(bowl.x, bowl.z, bowl.y + layout.bowl_height, layout.bowl_radius));
            }
            self.bowl.draw();
            program.set("uModel", Mat4::translation(lid) * Mat4::rotation_y(0.4 + 2.6 * i as f32));
            if material {
                program.set("uBowlShape", Vec4::default());
            }
            self.lid.draw();
        }
    }

    fn draw_shadow_map(&self) {
        self.shadow_framebuffer.bind();
        gfx::viewport(SHADOW_SIZE, SHADOW_SIZE);
        self.shadow_framebuffer.clear_depth(1.0);
        gfx::enable(gl::DEPTH_TEST);
        gfx::depth_func(gl::LESS);
        gfx::depth_mask(true);
        gfx::disable(gl::CULL_FACE);
        gfx::enable(gl::POLYGON_OFFSET_FILL);
        gfx::polygon_offset(1.5, 3.0);

        let program = &self.shadow_program;
        program.bind();
        program.set("uInstanced", 0);
        program.set("uModel", Mat4::IDENTITY);
        self.board_sides.draw();
        self.board_top.draw();
        self.draw_bowls(program, false);
        // The opaque stones lie together at the start of the buffer.
        program.set("uInstanced", 1);
        program.set("uFirst", 0);
        self.simple_stone.draw_instances(self.opaque[0].count + self.opaque[1].count);
        gfx::disable(gl::POLYGON_OFFSET_FILL);
    }

    fn draw_reflections(&self, scene: &Scene) {
        self.reflection_framebuffer.bind();
        gfx::viewport((self.width / 2).max(1), (self.height / 2).max(1));
        self.reflection_framebuffer.clear_color([0.0; 4]);
        self.reflection_framebuffer.clear_depth(1.0);
        if self.theme.reflection <= 0.0 {
            return;
        }

        // The stones turned upside down under the board, seen from the camera, as if from below the board.
        self.set_pass(vec3(scene.eye.x, -scene.eye.y, scene.eye.z), false, true);
        self.bind_lighting(false);
        gfx::enable(gl::CLIP_DISTANCE0);
        gfx::enable(gl::DEPTH_TEST);
        gfx::depth_func(gl::LESS);
        gfx::enable(gl::CULL_FACE);
        gfx::cull_face(gl::BACK);
        gfx::front_face(gl::CW);
        self.draw_stones(&self.opaque, false);
        gfx::front_face(gl::CCW);
        gfx::disable(gl::CLIP_DISTANCE0);
        self.reflection_color.generate_mipmap();
    }

    fn set_stone_material(&self, white: bool) {
        let look = if white { &self.theme.white } else { &self.theme.black };
        let program = &self.stone_program;
        program.set("uAlbedo", look.albedo);
        program.set("uSurface", vec4(look.roughness, look.metallic, look.clearcoat, look.clearcoat_roughness));
        program.set("uStyle", vec4(look.subsurface, look.pattern_strength, look.pattern, if white { 0.0 } else { 1.0 }));
        program.set("uTrim", look.trim.extend(look.trim_strength));
        program.set("uEdge", look.edge.extend(look.edge_strength));
    }

    fn draw_stones(&self, ranges: &[StoneRange], detailed: bool) {
        self.stone_program.bind();
        for range in ranges.iter().filter(|range| range.count > 0) {
            self.set_stone_material(range.white);
            self.stone_program.set("uFirst", range.first as i32);
            if detailed { &self.stone } else { &self.simple_stone }.draw_instances(range.count);
        }
    }

    fn set_surface_material(&self, scene: &Scene) {
        let p = &self.surface_program;
        let t = &self.theme;
        let l = &self.layout;
        let flag = |on: bool| if on { 1.0 } else { 0.0 };
        p.set("uBoardColor", t.board_color);
        p.set("uBoardSurface", vec4(t.board_roughness, t.board_clearcoat, t.board_clearcoat_roughness, t.board_pattern));
        p.set("uBoardReflection", vec4(t.reflection, t.reflection_blur, 0.0, 0.0));
        p.set("uGridColor", t.grid_color);
        p.set("uGrid", vec4(t.grid_width, t.grid_metallic, t.grid_roughness, t.grid_glow));
        p.set("uBoardShape", vec4(l.size as f32, l.half, l.board_half, t.star_radius));
        p.set("uBowlColor", t.bowl_color);
        p.set("uBowlSurface", vec4(t.bowl_roughness, t.bowl_clearcoat, t.bowl_pattern, t.bowl_seed));
        p.set("uTableColor", t.table_color);
        p.set("uTableSurface", vec4(t.table_roughness, t.table_pattern, t.table_sheen, t.table_glow));
        p.set("uTableFade", vec2(l.extent() + 1.5, l.extent() + 14.0));
        p.set("uMarkStyle", vec4(flag(scene.estimate), flag(scene.coordinates), 0.0, 0.0));
        // The soft shade round the feet of the bowls and the lids.
        let feet = [
            vec4(l.bowls[0].x, l.bowls[0].z, l.bowl_radius * 0.68, 0.65),
            vec4(l.bowls[1].x, l.bowls[1].z, l.bowl_radius * 0.68, 0.65),
            vec4(l.lids[0].x, l.lids[0].z, l.bowl_radius * 0.95, 0.5),
            vec4(l.lids[1].x, l.lids[1].z, l.bowl_radius * 0.95, 0.5),
        ];
        p.set("uDiscs", &feet[..]);
        p.set("uDiscCount", feet.len() as i32);
    }

    fn draw_main_pass(&self, scene: &Scene) {
        let fog = self.theme.fog;
        self.scene_framebuffer.bind();
        gfx::viewport(self.width, self.height);
        self.scene_framebuffer.clear_color([fog.x, fog.y, fog.z, 1.0]);
        self.scene_framebuffer.clear_depth(1.0);

        self.set_pass(scene.eye, true, false);
        self.bind_lighting(true);
        gfx::enable(gl::DEPTH_TEST);
        gfx::depth_func(gl::LESS);
        gfx::depth_mask(true);
        gfx::enable(gl::CULL_FACE);
        gfx::cull_face(gl::BACK);

        let surface = &self.surface_program;
        self.set_surface_material(scene);
        surface.bind();
        surface.set("uModel", Mat4::IDENTITY);
        surface.set("uKind", 2);
        self.table.draw();
        surface.set("uKind", 1);
        self.board_sides.draw();
        surface.set("uKind", 0);
        self.board_top.draw();
        surface.set("uKind", 3);
        self.draw_bowls(surface, true);

        self.draw_stones(&self.opaque, true);

        gfx::depth_func(gl::LEQUAL);
        gfx::depth_mask(false);
        self.background_program.bind();
        self.empty_vertex_array.bind();
        gfx::draw_triangles(3);

        // The stones seen through, blended over the rest.
        if !self.translucent.is_empty() {
            gfx::depth_func(gl::LESS);
            gfx::enable(gl::BLEND);
            gfx::blend_func(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            self.draw_stones(&self.translucent, true);
            gfx::disable(gl::BLEND);
        }

        gfx::depth_mask(true);
        gfx::depth_func(gl::LESS);
    }

    fn post_process(&self, scene: &Scene) {
        gfx::disable(gl::DEPTH_TEST);
        gfx::disable(gl::CULL_FACE);
        self.empty_vertex_array.bind();
        self.resolved_framebuffer.bind();
        gfx::viewport(self.width, self.height);
        self.resolve_program.set("uSamples", self.samples);
        self.resolve_program.bind();
        self.scene_color.bind(0);
        gfx::draw_triangles(3);

        // Bloom: halve down to a small image, then add the levels back up.
        self.bloom_down_program.bind();
        let (mut source, mut source_width, mut source_height) = (&self.resolved, self.width, self.height);
        for (level, bloom) in self.bloom.iter().enumerate() {
            bloom.framebuffer.bind();
            gfx::viewport(bloom.width, bloom.height);
            source.bind(0);
            self.bloom_down_program.set("uSourceTexel", vec2(1.0 / source_width as f32, 1.0 / source_height as f32));
            self.bloom_down_program.set("uFirst", i32::from(level == 0));
            gfx::draw_triangles(3);
            (source, source_width, source_height) = (&bloom.texture, bloom.width, bloom.height);
        }

        self.bloom_up_program.bind();
        gfx::enable(gl::BLEND);
        gfx::blend_func(gl::ONE, gl::ONE);
        for level in (0..BLOOM_LEVELS - 1).rev() {
            let (target, smaller) = (&self.bloom[level], &self.bloom[level + 1]);
            target.framebuffer.bind();
            gfx::viewport(target.width, target.height);
            smaller.texture.bind(0);
            self.bloom_up_program.set("uSourceTexel", vec2(1.0 / smaller.width as f32, 1.0 / smaller.height as f32));
            gfx::draw_triangles(3);
        }
        gfx::disable(gl::BLEND);

        gfx::bind_window_framebuffer();
        gfx::viewport(self.width, self.height);
        let composite = &self.composite_program;
        composite.set("uExposure", self.theme.exposure);
        composite.set("uBloomStrength", self.theme.bloom);
        composite.set("uVignette", self.theme.vignette);
        let (mut panels, mut styles) = ([Vec4::default(); MAX_FROSTED], [Vec4::default(); MAX_FROSTED]);
        let count = scene.panels.len().min(MAX_FROSTED);
        for (i, panel) in scene.panels.iter().take(count).enumerate() {
            panels[i] = vec4(panel.x + panel.width / 2.0, panel.y + panel.height / 2.0, panel.width / 2.0, panel.height / 2.0);
            styles[i] = vec4(panel.radius, panel.opacity, 0.0, 0.0);
        }
        composite.set("uPanels", &panels[..]);
        composite.set("uPanelStyle", &styles[..]);
        composite.set("uPanelCount", count as i32);
        composite.bind();
        self.resolved.bind(0);
        self.bloom[0].texture.bind(1);
        self.bloom[2].texture.bind(2);
        gfx::draw_triangles(3);
    }
}
