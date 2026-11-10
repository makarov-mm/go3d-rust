// The board with its lines, hoshi, coordinates and marks, its sides, the table, and the turned bowls and lids.

in vec3 vWorld;
in vec3 vNormal;
in vec2 vUv;

uniform int uKind; // 0 the top of the board, 1 its sides, 2 the table, 3 bowls and lids

uniform vec3 uBoardColor;
uniform vec4 uBoardSurface;    // roughness, clearcoat, clearcoat roughness, pattern (0 kaya, 1 bamboo, 2 slate, 3 glass)
uniform vec4 uBoardReflection; // x: reflection strength, y: reflection blur
uniform vec3 uGridColor;
uniform vec4 uGrid;            // x: half the width of a line, y: metallic, z: roughness, w: glow
uniform vec4 uBoardShape;      // x: lines, y: from the middle to the outer lines, z: to the edge, w: radius of the hoshi
uniform vec3 uBowlColor;
uniform vec4 uBowlSurface;     // roughness, clearcoat, pattern (0 wood, 1 lacquer), wood seed
uniform vec3 uTableColor;
uniform vec4 uTableSurface;    // roughness, pattern (0 felt, 1 wood, 2 neon grid, 3 stone), sheen, grid glow
uniform vec2 uTableFade;       // the table darkens between these distances from the middle
uniform vec4 uBowlShape;       // of the bowl drawn: the middle (xz), the height of its rim, its radius; 0 for a lid
uniform vec4 uMarkStyle;       // x: the territory shown faintly, as an estimate; y: 1 to show the coordinates
uniform vec4 uDiscs[8];        // the feet of the bowls and lids: xz, radius, darkness
uniform int uDiscCount;

layout(binding = 5) uniform sampler2D uReflection;
layout(binding = 6) uniform sampler2D uLabels;
layout(binding = 7) uniform usampler2D uMarks; // for each point: territory, hover, hint, ko, stone

layout(location = 0) out vec4 fragColor;

const uint BlackArea = 1u, WhiteArea = 2u, Hover = 4u, Hint = 8u, Ko = 16u, StoneHere = 32u;

// Wood with its grain along x: growth lines of uneven spacing and strength, fine fibres and a soft figure.
// Detail finer than the footprint, the size of a pixel in p, fades to its average instead of flickering.
vec3 Wood(vec2 p, vec3 color, float seed, float footprint)
{
    p += vec2(seed * 31.7, seed * 17.3);
    // The growth lines: their spacing drifts across the grain, and they wander a little along it.
    float t = p.y * 7.0 + 2.5 * Fbm(vec2(p.y * 0.8, seed * 5.0)) + 0.8 * Fbm(vec2(p.x * 0.3, p.y * 1.2)) + 0.2 * sin(p.x * 1.1 + seed * 6.0);
    float ring = floor(t);
    float phase = t - ring;
    float width = 0.2 + 0.3 * Hash12(vec2(ring, seed));
    float late = smoothstep(0.0, 0.08, phase) * (1.0 - smoothstep(width, width + 0.25, phase));
    late *= 0.35 + 0.9 * Hash12(vec2(ring, seed + 7.0));
    late = mix(late, 0.3, smoothstep(0.04, 0.12, footprint));
    float fibers = 0.5 * Noise(vec2(p.x * 3.0, p.y * 260.0)) + 0.5 * Noise(vec2(p.x * 7.0, p.y * 600.0));
    fibers = mix(fibers, 0.5, smoothstep(0.001, 0.004, footprint));
    float figure = Noise(p * vec2(0.8, 2.2) + seed * 9.0);
    return color * (1.0 - 0.2 * late) * (0.9 + 0.16 * fibers) * (0.9 + 0.2 * figure);
}

vec3 Marble(vec2 p, vec3 color, vec3 veinColor, float seed)
{
    p += seed * 10.0;
    float warp = Fbm(p * 1.3) * 4.5;
    float wave = abs(sin((p.x * 1.4 + p.y * 0.9 + warp) * 2.6));
    float vein = pow(1.0 - wave, 14.0) + 0.35 * pow(1.0 - abs(sin((p.x * 3.1 - p.y * 2.2 + warp * 1.7) * 3.0)), 30.0);
    vec3 c = color * (0.9 + 0.2 * Fbm(p * 2.7 + 5.0));
    return mix(c, veinColor, Saturate(vein) * 0.75);
}

// Felt: fine fibres and a soft mottling, fading to their average where they are finer than a pixel.
vec3 Felt(vec2 p, vec3 color, float footprint)
{
    float fine = mix(Noise(p * 260.0), 0.5, smoothstep(0.002, 0.006, footprint));
    float coarse = mix(Noise(p * 90.0 + 3.0), 0.5, smoothstep(0.006, 0.018, footprint));
    return color * (0.84 + 0.3 * (0.6 * fine + 0.4 * coarse)) * (0.92 + 0.16 * Fbm(p * 0.9));
}

// The top of the board: kaya with its fine straight grain, laminated bamboo, slate or dark glass.
vec3 BoardAlbedo(vec2 p, float footprint, int pattern)
{
    if (pattern == 0)
        return Wood(vec2(p.y, p.x) * vec2(0.25, 0.9), uBoardColor, 0.53, footprint);
    if (pattern == 1) {
        // Strips of bamboo along the board, each its own shade, with fine fibres and darker joints.
        float strip = floor(p.x / 0.36);
        float across = fract(p.x / 0.36);
        vec3 c = uBoardColor * (0.9 + 0.2 * Hash12(vec2(strip, 1.7)));
        float fibers = mix(Noise(vec2(p.x * 90.0, p.y * 4.0)), 0.5, smoothstep(0.004, 0.012, footprint));
        float joint = 1.0 - smoothstep(0.0, 0.05, min(across, 1.0 - across));
        float knot = smoothstep(0.85, 1.0, sin(p.y * 0.9 + strip * 2.3) * 0.5 + 0.5);
        return c * (0.92 + 0.16 * fibers) * (1.0 - 0.18 * mix(joint, 0.3, smoothstep(0.02, 0.06, footprint))) * (1.0 - 0.12 * knot);
    }
    if (pattern == 2)
        return Marble(p * 0.21, uBoardColor, uBoardColor * 3.0 + 0.04, 3.0);
    return uBoardColor * (0.9 + 0.2 * Noise(p * 6.0));
}

bool IsStar(ivec2 point, int lines)
{
    int edge = lines >= 13 ? 3 : 2, far = lines - 1 - edge, middle = lines / 2;
    bool x = point.x == edge || point.x == far, y = point.y == edge || point.y == far;
    if (lines >= 19) {
        x = x || point.x == middle;
        y = y || point.y == middle;
    }
    bool center = (lines & 1) == 1 && point.x == middle && point.y == middle;
    return (x && y) || center;
}

float ContactShadow(vec2 xz)
{
    float shade = 1.0;
    for (int i = 0; i < uDiscCount; ++i) {
        vec4 disc = uDiscs[i];
        float d = length(xz - disc.xy);
        shade *= 1.0 - disc.w * (1.0 - smoothstep(disc.z * 0.55, disc.z * 1.9, d));
    }
    return shade;
}

void main()
{
    vec3 N = normalize(vNormal);
    vec3 V = normalize(uEye.xyz - vWorld);
    float time = uCameraPosition.w;
    Material m = DefaultMaterial(vec3(0.5), 0.5);
    vec3 glow = vec3(0.0);
    vec4 planar = vec4(0.0);

    if (uKind == 0) {
        int lines = int(uBoardShape.x + 0.5);
        vec2 grid = vWorld.xz + uBoardShape.y;        // the lines at whole numbers, 0 to lines - 1
        vec2 pixel = max(fwidth(grid), vec2(1e-5));   // the size of a pixel on the board
        float footprint = length(pixel);
        float soft = max(0.01, 0.75 * footprint);    // sharp edges softened to a pixel when the board is far
        float pulse = 0.5 + 0.5 * sin(time * 3.5);
        int pattern = int(uBoardSurface.w + 0.5);
        m.albedo = BoardAlbedo(vWorld.xz, footprint, pattern);
        m.roughness = uBoardSurface.x;
        m.clearcoat = uBoardSurface.y;
        m.clearcoatRoughness = uBoardSurface.z;

        // The nearest point and the marks round it: the stones darken the board under and beside them.
        float last = float(lines - 1);
        ivec2 nearest = ivec2(clamp(round(grid), vec2(0.0), vec2(last)));
        float shade = 1.0;
        for (int dy = -1; dy <= 1; ++dy) {
            for (int dx = -1; dx <= 1; ++dx) {
                ivec2 point = nearest + ivec2(dx, dy);
                if (point.x < 0 || point.y < 0 || point.x >= lines || point.y >= lines)
                    continue;
                if ((texelFetch(uMarks, point, 0).r & StoneHere) != 0u)
                    shade *= 1.0 - 0.5 * (1.0 - smoothstep(0.3, 0.72, length(grid - vec2(point))));
            }
        }
        uint marks = texelFetch(uMarks, nearest, 0).r;
        vec2 local = grid - vec2(nearest);
        float d = length(local);
        vec2 box = abs(local);

        // The lines, the outer ones thicker, and the hoshi, averaged over the pixel.
        vec2 offset = grid - round(grid);
        vec2 lineIndex = round(grid);
        vec2 width = vec2(uGrid.x) * mix(vec2(1.0), vec2(1.7), vec2(equal(lineIndex, vec2(0.0))) + vec2(equal(lineIndex, vec2(last))));
        // Each line runs between the outer lines and no further.
        float reach = uGrid.x * 1.7;
        bool withinRows = grid.y > -reach && grid.y < last + reach, withinColumns = grid.x > -reach && grid.x < last + reach;
        float line = 0.0;
        if (withinRows && lineIndex.x >= 0.0 && lineIndex.x <= last)
            line = max(line, LineCoverage(abs(offset.x), width.x, pixel.x));
        if (withinColumns && lineIndex.y >= 0.0 && lineIndex.y <= last)
            line = max(line, LineCoverage(abs(offset.y), width.y, pixel.y));
        if (IsStar(nearest, lines))
            line = max(line, 1.0 - smoothstep(uBoardShape.w - 0.6 * footprint, uBoardShape.w + 0.6 * footprint, d));
        // The coordinates, written round the lines.
        float label = uMarkStyle.y > 0.5 ? texture(uLabels, (vWorld.xz + uBoardShape.z) / (2.0 * uBoardShape.z)).r * 0.85 : 0.0;
        float ink = max(line, label);
        m.albedo = mix(m.albedo, uGridColor, ink);
        m.metallic = mix(m.metallic, uGrid.y, ink);
        m.roughness = mix(m.roughness, uGrid.z, ink);
        glow += uGridColor * uGrid.w * ink;

        // Territory, as small squares; the point under the pointer, a hint, and a ko that may not be taken back yet.
        float square = uMarkStyle.x > 0.5 ? 0.13 : 0.17;
        float inSquare = (1.0 - smoothstep(square - soft, square + soft, box.x)) * (1.0 - smoothstep(square - soft, square + soft, box.y));
        float strength = uMarkStyle.x > 0.5 ? 0.75 : 1.0;
        // A thin rim keeps a square of the board's own darkness or lightness in sight: light round black squares
        // on a dark board, dark round white squares on a light one.
        float rimEdge = square + 0.03;
        float outer = (1.0 - smoothstep(rimEdge - soft, rimEdge + soft, box.x)) * (1.0 - smoothstep(rimEdge - soft, rimEdge + soft, box.y));
        float rim = max(outer - inSquare, 0.0);
        float darkBoard = 1.0 - smoothstep(0.04, 0.15, Luminance(uBoardColor));
        if ((marks & BlackArea) != 0u)
            m.albedo = mix(mix(m.albedo, vec3(0.45), rim * darkBoard * strength), vec3(0.015), inSquare * strength);
        if ((marks & WhiteArea) != 0u)
            m.albedo = mix(mix(m.albedo, vec3(0.02), rim * (1.0 - darkBoard) * 0.7 * strength), vec3(0.86, 0.85, 0.82), inSquare * strength);
        if ((marks & Hover) != 0u)
            glow += uAccent.rgb * (1.0 - smoothstep(0.016 - soft, 0.016 + soft, abs(d - 0.52))) * 1.2;
        if ((marks & Hint) != 0u)
            glow += uAccent.rgb * (1.0 - smoothstep(0.024 - soft, 0.024 + soft, abs(d - 0.5 - 0.04 * pulse))) * (1.6 + 0.8 * pulse);
        if ((marks & Ko) != 0u) {
            float outline = abs(max(box.x, box.y) - 0.2);
            glow += uAccentWarm.rgb * (1.0 - smoothstep(0.015 - soft, 0.015 + soft, outline)) * 1.4;
        }

        // The stones mirrored in the polish, added over the shading below.
        vec2 screen = gl_FragCoord.xy * uViewport.zw + N.xz * 0.012;
        planar = textureLod(uReflection, screen, uBoardReflection.y);
        m.occlusion = shade;
    }
    else if (uKind == 1) {
        // The sides of the board: the same wood, its grain running round.
        int pattern = int(uBoardSurface.w + 0.5);
        bool alongX = abs(vWorld.z) > abs(vWorld.x);
        vec2 along = alongX ? vec2(vWorld.x, vWorld.y * 3.0 + vWorld.z) : vec2(vWorld.z, vWorld.y * 3.0 + vWorld.x);
        float footprint = length(fwidth(along));
        m.albedo = pattern == 0 || pattern == 1 ? Wood(along * vec2(0.4, 1.0), uBoardColor * 0.92, 0.29, footprint) : BoardAlbedo(along, footprint, pattern);
        m.roughness = uBoardSurface.x;
        m.clearcoat = uBoardSurface.y;
        m.clearcoatRoughness = uBoardSurface.z + 0.02;
        m.occlusion = mix(0.6, 1.0, smoothstep(-uBoardShape.z * 0.1 - 0.8, -0.1, vWorld.y));
    }
    else if (uKind == 3) {
        // Turned wood: the rings follow the turning, the grain runs round.
        vec2 turned = vec2(vUv.y * 40.0, vUv.x * 3.0);
        float footprint = length(fwidth(turned));
        m.albedo = Wood(turned, uBowlColor, uBowlSurface.w, footprint * 0.5);
        if (uBowlSurface.z > 0.5)
            m.albedo = uBowlColor * (0.92 + 0.08 * Noise(turned * 0.4));
        m.roughness = uBowlSurface.x;
        m.clearcoat = uBowlSurface.y;
        m.clearcoatRoughness = 0.08;
        m.occlusion = mix(0.55, 1.0, Saturate(N.y * 0.5 + 0.6));
        // Inside a bowl, darker the deeper.
        if (uBowlShape.w > 0.0 && length(vWorld.xz - uBowlShape.xy) < 0.79 * uBowlShape.w && vWorld.y > uBowlShape.z - 0.3 * uBowlShape.w)
            m.occlusion = mix(1.0, 0.35, smoothstep(0.0, 0.22 * uBowlShape.w, uBowlShape.z - vWorld.y));
    }
    else {
        int pattern = int(uTableSurface.y + 0.5);
        vec2 p = vWorld.xz;
        float distance = length(p);
        float footprint = length(fwidth(p));
        if (pattern == 0) {
            m.albedo = Felt(p, uTableColor, footprint);
        }
        else if (pattern == 1) {
            m.albedo = Wood(p * vec2(0.25, 0.9), uTableColor, 0.81, footprint * 0.9);
        }
        else if (pattern == 2) {
            // The glowing grid of the first version of the game.
            m.albedo = uTableColor;
            vec2 grid = abs(fract(p - 0.5) - 0.5) / fwidth(p);
            float minor = 1.0 - Saturate(min(grid.x, grid.y) - 0.5);
            vec2 major2 = abs(fract(p * 0.2 - 0.5) - 0.5) / fwidth(p * 0.2);
            float major = 1.0 - Saturate(min(major2.x, major2.y) - 0.8);
            float fade = exp(-distance * 0.16);
            glow += uAccent.rgb * (minor * 0.35 + major * 1.1) * fade * uTableSurface.w;
        }
        else {
            m.albedo = Marble(p * 0.22, uTableColor, uTableColor * 1.7, 6.0);
        }
        m.roughness = uTableSurface.x;
        // The light falls on the board; the table darkens away from it.
        m.albedo *= mix(1.0, 0.18, smoothstep(uTableFade.x, uTableFade.y, distance));
        m.occlusion = ContactShadow(p);
        // Darker under and around the board.
        float board = max(abs(p.x), abs(p.y));
        m.occlusion *= mix(0.35, 1.0, smoothstep(uBoardShape.z - 0.05, uBoardShape.z + 1.3, board));
        if (uTableSurface.z > 0.0) {
            // The sheen of cloth at grazing angles.
            m.emissive += uTableColor * uTableSurface.z * pow(1.0 - Saturate(dot(N, V)), 4.0) * 0.5;
        }
    }

    float spread = NormalSpread(N);
    m.roughness = WidenRoughness(m.roughness, spread);
    m.clearcoatRoughness = WidenRoughness(m.clearcoatRoughness, spread);
    float shadow = Shadow(vWorld, N);
    vec3 color = Shade(m, vWorld, N, V, shadow) + glow;
    if (uKind == 0) {
        float fresnel = mix(0.1, 1.0, pow(1.0 - Saturate(dot(N, V)), 5.0));
        color = mix(color, planar.rgb, planar.a * fresnel * uBoardReflection.x);
    }
    fragColor = vec4(ApplyFog(color, vWorld), 1.0);
}
