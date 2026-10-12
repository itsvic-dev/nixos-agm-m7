#version 450

void main()
{
	vec2 pos[3] = vec2[](vec2(0.0, -0.75), vec2(0.75, 0.75), vec2(-0.75, 0.75));
	gl_Position = vec4(pos[gl_VertexIndex], 0.0, 1.0);
}
