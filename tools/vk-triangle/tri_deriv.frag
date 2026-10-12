#version 450

layout(location = 0) out vec4 color;

void main()
{
	color = vec4(dFdx(gl_FragCoord.x), dFdy(gl_FragCoord.y), 0.0, 1.0);
}
