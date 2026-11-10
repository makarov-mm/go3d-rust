// A stone: slate or shell, glass, yunzi or acrylic, with a ring on the stone played last, a glow, a dissolve,
// and seen through when it is dead or shows where a click would play.

in vec3 vWorld;
in vec3 vNormal;
in vec3 vLocal;
flat in vec4 vGlow;
flat in vec4 vParams;
flat in float vOpacity;

uniform vec3 uAlbedo;
uniform vec4 uSurface; // roughness, metallic, clearcoat, clearcoat roughness
uniform vec4 uStyle;   // x: subsurface, y: pattern strength, z: pattern (0 none, 1 shell, 2 slate, 3 glass, 4 yunzi), w: 1 for black
uniform vec4 uTrim;    // rgb, strength: a glowing ring round the edge
uniform vec4 uEdge;    // rgb, strength: light coming through the thin edge

layout(location = 0) out vec4 fragColor;

const float StoneRadius = 0.475;
const float HalfHeight = 0.19;

void main()
{
    // Measured before anything is discarded, while the neighbouring pixels still run.
    vec3 N = normalize(vNormal);
    float spread = NormalSpread(N);
    vec3 localPixel = fwidth(vLocal);
    float radiusPixel = max(fwidth(length(vLocal.xz)), 1e-5);

    float seed = vParams.y;
    vec3 emissive = vec3(0.0);
    if (vParams.x > 0.0) {
        float n = Fbm(vLocal * 9.0 + 3.0 + seed * 5.0);
        float threshold = vParams.x * 1.2 - 0.1;
        if (n < threshold)
            discard;
        emissive += uAccent.rgb * (1.0 - smoothstep(0.0, 0.06, n - threshold)) * 8.0;
    }

    if (!gl_FrontFacing)
        N = -N;
    vec3 V = normalize(uEye.xyz - vWorld);

    Material m = DefaultMaterial(uAlbedo, uSurface.x);
    m.metallic = uSurface.y;
    m.clearcoat = uSurface.z;
    m.clearcoatRoughness = uSurface.w;
    m.subsurface = uStyle.x;

    float radius = length(vLocal.xz);
    float edge = radius / StoneRadius; // 0 in the middle, 1 at the edge
    int pattern = int(uStyle.z + 0.5);
    float footprint = length(localPixel);
    if (pattern == 1) {
        // Clamshell: fine growth lines across the face, gently bowed, fading where finer than a pixel.
        vec3 q = vLocal + vec3(seed * 0.3, 0.0, seed * 0.7);
        float bands = q.z * 70.0 + 9.0 * q.x * q.x + 1.5 * Noise(q.xz * 6.0 + seed * 9.0);
        float line = 0.5 + 0.5 * sin(bands);
        float fine = mix(line, 0.5, smoothstep(0.006, 0.02, footprint));
        float broad = Noise(vec2(q.z * 9.0, seed * 13.0));
        m.albedo *= 1.0 - uStyle.y * (0.06 * fine + 0.05 * broad);
    }
    else if (pattern == 2) {
        // Slate: matte, with a faint mottle and fine grain.
        float mottle = Fbm(vLocal * 9.0 + seed * 11.0);
        float grain = mix(Noise(vLocal.xz * 260.0 + seed), 0.5, smoothstep(0.002, 0.006, footprint));
        m.albedo *= 0.85 + 0.3 * uStyle.y * mottle + 0.1 * uStyle.y * (grain - 0.5);
        m.roughness = clamp(m.roughness + 0.1 * (mottle - 0.5), 0.05, 1.0);
    }
    else if (pattern == 3) {
        // Glass: a swirl deep inside.
        float swirl = Fbm(vLocal * 5.0 + seed * 7.0);
        m.albedo *= 0.85 + 0.3 * uStyle.y * swirl;
    }
    else if (pattern == 4) {
        // Yunzi: soft and even, with a faint cloud.
        m.albedo *= 0.93 + 0.14 * uStyle.y * Fbm(vLocal * 4.0 + seed * 3.0);
    }

    // The stone played last carries a ring on its upper face, in the color of the other side.
    float mark = 0.0;
    if (vParams.w > 0.0 && vLocal.y > 0.0) {
        mark = LineCoverage(abs(radius - 0.19), 0.03, radiusPixel) * vParams.w;
        vec3 ring = uStyle.w > 0.5 ? vec3(0.85, 0.84, 0.8) : vec3(0.03, 0.03, 0.035);
        m.albedo = mix(m.albedo, ring, mark);
        m.roughness = mix(m.roughness, 0.4, mark);
        m.metallic *= 1.0 - mark;
    }

    // The ground darkens what faces it, near the bottom most.
    float above = vWorld.y - vParams.z;
    float ground = mix(0.35, 1.0, smoothstep(0.0, 0.3, above));
    m.occlusion = mix(1.0, ground, Saturate(0.6 - N.y * 0.6));

    // Light through the thin edge, for yunzi and glass; a neon ring for the acrylic set.
    float rim = smoothstep(0.75, 1.0, edge);
    m.emissive = emissive + uEdge.rgb * uEdge.a * rim * Saturate(dot(-V, normalize(uLightDirection.xyz)) * 0.5 + 0.5);
    m.emissive += uTrim.rgb * uTrim.a * LineCoverage(abs(vLocal.y), 0.012, max(localPixel.y, 1e-5)) * step(0.9, edge);

    m.roughness = WidenRoughness(m.roughness, spread);
    m.clearcoatRoughness = WidenRoughness(m.clearcoatRoughness, spread);
    float shadow = uEye.w > 0.5 ? 1.0 : Shadow(vWorld, N);
    vec3 color = Shade(m, vWorld, N, V, shadow);

    // The glow of stones about to be taken: a halo at the silhouette and some light all over.
    float NoV = Saturate(dot(N, V));
    color += vGlow.rgb * vGlow.a * (0.22 + 1.6 * pow(1.0 - NoV, 3.0));

    fragColor = vec4(ApplyFog(color, vWorld), vOpacity);
}
