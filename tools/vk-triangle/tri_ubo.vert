#version 450

layout(binding = 0) uniform Positions {
	vec4 pos[3];
};

void main()
{
	gl_Position = pos[gl_VertexIndex];
}
