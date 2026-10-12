#version 450

layout(location = 0) out vec4 texcoord;
layout(location = 1) out vec3 frag_pos;

void main()
{
	vec2 pos[3] = vec2[](vec2(0.0, -0.75), vec2(0.75, 0.75), vec2(-0.75, 0.75));
	gl_Position = vec4(pos[gl_VertexIndex], 0.5, 1.0);
	texcoord = vec4(pos[gl_VertexIndex] * 0.5 + 0.5, 0.0, 1.0);
	frag_pos = gl_Position.xyz;
}
