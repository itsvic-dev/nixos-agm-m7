#version 450

layout(push_constant) uniform Params {
	vec2 size;
	int mode;
} params;

layout(location = 0) out vec2 pix;
layout(location = 1) centroid out vec2 cpix;
layout(location = 2) sample out vec2 spix;

void main()
{
	vec2 corner[6] = vec2[](vec2(-1, -1), vec2(1, -1), vec2(-1, 1),
				vec2(-1, 1), vec2(1, -1), vec2(1, 1));
	vec2 pos = corner[gl_VertexIndex];
	gl_Position = vec4(pos, 0.0, 1.0);
	pix = (pos * 0.5 + 0.5) * params.size;
	cpix = pix;
	spix = pix;
}
