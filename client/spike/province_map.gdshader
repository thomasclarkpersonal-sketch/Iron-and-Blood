// Province map (D12): the texture stores each pixel's province index as r + 256·g;
// `lut` holds one colour per province. A map-mode change rewrites only `lut`.
shader_type canvas_item;

uniform sampler2D lut : filter_nearest;
uniform vec4 border_color : source_color = vec4(0.08, 0.08, 0.1, 1.0);

int province_at(sampler2D ids, vec2 uv) {
	vec4 c = texture(ids, uv);
	return int(round(c.r * 255.0)) + 256 * int(round(c.g * 255.0));
}

void fragment() {
	int id = province_at(TEXTURE, UV);
	vec2 px = TEXTURE_PIXEL_SIZE;
	bool border = id != province_at(TEXTURE, UV + vec2(px.x, 0.0)) || id != province_at(TEXTURE, UV + vec2(0.0, px.y));
	COLOR = border ? border_color : texelFetch(lut, ivec2(id, 0), 0);
}
