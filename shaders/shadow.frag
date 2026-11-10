// Depth only; a dissolving stone casts its remaining part.

in vec3 vLocal;
flat in vec2 vDissolve;

void main()
{
    if (vDissolve.x > 0.0 && Fbm(vLocal * 9.0 + 3.0 + vDissolve.y * 5.0) < vDissolve.x * 1.2 - 0.1)
        discard;
}
