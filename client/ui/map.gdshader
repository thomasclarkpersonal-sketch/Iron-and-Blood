// Province map (D12, M3-8b). TEXTURE is the province-ID image from the bridge: each
// texel's r + 256·g is its province index + 1, and 0 is background (sea). This mirrors
// pax_map's id encoding (BACKGROUND_ID, province_of) and pax_godot::map's texels: the
// one copy outside Rust; keep it in step. `lut` holds one colour per province, so a
// map-mode change rewrites only `lut`.
shader_type canvas_item;

uniform sampler2D lut : filter_nearest;
uniform vec4 background : source_color = vec4(0.13, 0.22, 0.34, 1.0);
uniform vec4 border_color : source_color = vec4(0.06, 0.06, 0.08, 1.0);
// The selected province's index + 1 (0: none), highlighted.
uniform int selected_id = 0;

// The province index + 1 a texel encodes.
int id_of(vec4 texel) {
	return int(round(texel.r * 255.0)) + 256 * int(round(texel.g * 255.0));
}

void fragment() {
	vec2 px = TEXTURE_PIXEL_SIZE;
	int id = id_of(texture(TEXTURE, UV));
	int right = id_of(texture(TEXTURE, UV + vec2(px.x, 0.0)));
	int below = id_of(texture(TEXTURE, UV + vec2(0.0, px.y)));
	vec4 c = id == 0 ? background : texelFetch(lut, ivec2(id - 1, 0), 0);
	if (id != 0 && id == selected_id) {
		c = mix(c, vec4(1.0), 0.35);
	}
	COLOR = id != right || id != below ? border_color : c;
}
