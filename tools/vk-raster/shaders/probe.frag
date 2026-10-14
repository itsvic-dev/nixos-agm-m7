#version 450

layout(push_constant) uniform Params {
	vec2 size;
	int mode;
} params;

layout(set = 0, binding = 0) uniform sampler2D tex;

layout(location = 0) in vec2 pix;
layout(location = 1) centroid in vec2 cpix;
layout(location = 2) sample in vec2 spix;
layout(location = 0) out vec4 color;

void main()
{
	vec2 fc = gl_FragCoord.xy;

	if (params.mode == 0)
		color = vec4(fract(fc), 0.0, 1.0);
	else if (params.mode == 1)
		color = vec4(fract(pix), (floor(pix) - floor(fc) + 8.0) / 16.0);
	else if (params.mode == 4)
		color = vec4(fc - floor(fc), pix - floor(pix));
	else if (params.mode == 5)
		color = vec4(mod(fc, 2.0) / 2.0, mod(pix, 2.0) / 2.0);
	else if (params.mode == 6)
		color = vec4(cpix - floor(cpix), spix - floor(spix));
	else if (params.mode == 2)
		color = texture(tex, pix / params.size);
	else
		color = texelFetch(tex, ivec2(pix), 0);
}
