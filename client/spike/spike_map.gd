## M3-0 spike (D12): decode a real Welcome and DayUpdate through the Rust bridge,
## then draw a province map coloured by the DayUpdate's map values with the
## province-ID shader. Needs the bridge built with `--features demo` (PaxDemo).
## Run with `-- --screenshot=<path>` to save an image and quit.
extends Node2D

const PROVINCES := 300
const MAP_SIZE := Vector2i(960, 500)


func _ready() -> void:
	var reader := PaxServerReader.new()
	var messages: Array = reader.push(PaxDemo.frames(PROVINCES))
	if reader.failed() or messages.size() != 2:
		_fail("decode failed: %s (%d messages)" % [reader.error(), messages.size()])
		return
	var welcome: Dictionary = messages[0]
	var update: Dictionary = messages[1]
	if welcome["type"] != "welcome" or update["type"] != "day_update":
		_fail("unexpected message types %s, %s" % [welcome["type"], update["type"]])
		return

	var provinces: PackedStringArray = welcome["provinces"]
	var values: PackedFloat32Array = update["map_values"]
	var ids := Image.create_from_data(MAP_SIZE.x, MAP_SIZE.y, false, Image.FORMAT_RGB8,
		PaxDemo.province_id_image(MAP_SIZE.x, MAP_SIZE.y, provinces.size()))

	var map := Sprite2D.new()
	map.texture = ImageTexture.create_from_image(ids)
	map.centered = false
	map.position = Vector2(0, 40)
	map.texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST
	var material := ShaderMaterial.new()
	material.shader = load("res://spike/province_map.gdshader")
	material.set_shader_parameter("lut", ImageTexture.create_from_image(_colour_lut(values)))
	map.material = material
	add_child(map)

	var label := Label.new()
	label.position = Vector2(12, 8)
	label.text = "%s: day %d, %d provinces in %d nations, population map mode %d, state %x" % [
		welcome["scenario"], update["day"], provinces.size(), (welcome["nations"] as PackedStringArray).size(),
		update["map_mode"], update["state_hash"]]
	add_child(label)
	print("SPIKE OK: %d provinces, %d map values, nations %s" % [provinces.size(), values.size(), welcome["nations"]])

	var shot := _arg("--screenshot=")
	if shot != "":
		await RenderingServer.frame_post_draw
		await RenderingServer.frame_post_draw
		get_viewport().get_texture().get_image().save_png(shot)
		print("SCREENSHOT %s" % shot)
		get_tree().quit()


## One colour per province: low values blue, high values red (display only).
func _colour_lut(values: PackedFloat32Array) -> Image:
	var lo := INF
	var hi := -INF
	for v in values:
		lo = minf(lo, v)
		hi = maxf(hi, v)
	var lut := Image.create(values.size(), 1, false, Image.FORMAT_RGBA8)
	for i in values.size():
		var t := 0.0 if hi <= lo else (values[i] - lo) / (hi - lo)
		lut.set_pixel(i, 0, Color.from_hsv(0.66 * (1.0 - t), 0.75, 0.9))
	return lut


func _arg(prefix: String) -> String:
	for a in OS.get_cmdline_user_args():
		if a.begins_with(prefix):
			return a.substr(prefix.length())
	return ""


func _fail(message: String) -> void:
	push_error("SPIKE FAILED: " + message)
	get_tree().quit(1)
