#version 450

layout(constant_id = 0) const int MODE = 0;

layout(location = 0) in vec4 texcoord;
layout(location = 1) in vec3 frag_pos;
layout(location = 0) out vec4 color;

const vec3 light_dir = vec3(0.424, 0.566, 0.707);

void main()
{
	vec3 dx = dFdx(frag_pos);
	vec3 dy = dFdy(frag_pos);

	if (MODE == 0)
		color = vec4(abs(dx.x) * 32.0, abs(dy.y) * 32.0, 0.0, 1.0);
	else if (MODE == 1)
		color = vec4(normalize(cross(dx, dy)) * 0.5 + 0.5, 1.0);
	else if (MODE == 2)
		color = vec4(vec3(max(0.0, dot(light_dir, normalize(cross(dx, dy))))), 1.0);
	else if (MODE == 3)
		color = vec4(pow(0.5, 1.0 / 2.4), 0.25, 0.0, 1.0);
	else if (MODE == 4)
		color = vec4(texcoord.xy, 0.0, 1.0);
	else if (MODE == 5)
		color = vec4(texcoord.zw, 0.0, 1.0);
	else if (MODE == 6)
		color = vec4(frag_pos * 0.5 + 0.5, 1.0);
	else if (MODE == 7)
		color = vec4(dx.x > 0.0 ? 1.0 : 0.0, dy.y > 0.0 ? 1.0 : 0.0, 0.0, 1.0);
	else
		color = vec4(dFdx(gl_FragCoord.x) > 0.0 ? 1.0 : 0.0, dFdy(gl_FragCoord.y) > 0.0 ? 1.0 : 0.0, 0.0, 1.0);
}
