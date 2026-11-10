// The stones, drawn many at once: each instance its own matrix and look. In the reflection pass, mirrored in the board.

layout(location = 0) in vec3 aPosition;
layout(location = 1) in vec3 aNormal;

struct Stone {
    mat4 model;
    vec4 glow;   // rgb, strength
    vec4 params; // x: dissolve, y: seed, z: height of the ground, w: the mark of the last move
    vec4 extra;  // x: opacity
};

layout(std430, binding = 0) readonly buffer Stones {
    Stone uStones[];
};

uniform int uFirst; // where the stones of this draw begin in the buffer

out vec3 vWorld;
out vec3 vNormal;
out vec3 vLocal;
flat out vec4 vGlow;
flat out vec4 vParams;
flat out float vOpacity;

void main()
{
    Stone stone = uStones[uFirst + gl_InstanceID];
    vec4 world = stone.model * vec4(aPosition, 1.0);
    vWorld = world.xyz;
    vNormal = mat3(stone.model) * aNormal;
    vLocal = aPosition;
    vGlow = stone.glow;
    vParams = stone.params;
    vOpacity = stone.extra.x;

    if (uMirror.y > 0.5) {
        // Only the stones over the board are mirrored in it.
        vec2 center = stone.model[3].xz;
        gl_ClipDistance[0] = min(world.y - uMirror.x, uMirror.z - max(abs(center.x), abs(center.y)));
        world.y = 2.0 * uMirror.x - world.y;
    }
    else {
        gl_ClipDistance[0] = 1.0;
    }
    gl_Position = uViewProjection * world;
}
