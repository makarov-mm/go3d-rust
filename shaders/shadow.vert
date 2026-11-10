// The depth of the stones, the board and the bowls as the key light sees them. Stones are drawn many at once.

layout(location = 0) in vec3 aPosition;

struct Stone {
    mat4 model;
    vec4 glow;
    vec4 params;
    vec4 extra;
};

layout(std430, binding = 0) readonly buffer Stones {
    Stone uStones[];
};

uniform mat4 uModel;
uniform int uInstanced;
uniform int uFirst;

out vec3 vLocal;
flat out vec2 vDissolve; // amount, seed

void main()
{
    vLocal = aPosition;
    mat4 model = uModel;
    vDissolve = vec2(0.0);
    if (uInstanced != 0) {
        model = uStones[uFirst + gl_InstanceID].model;
        vDissolve = uStones[uFirst + gl_InstanceID].params.xy;
    }
    gl_Position = uLightViewProjection * model * vec4(aPosition, 1.0);
}
