## The province map (M3-8b, D12): the bridge's province-ID texture drawn through
## `map.gdshader`, with province names, zoom (wheel), pan (right or middle drag) and
## selection (left click).
extends Control

const PaxKeys := preload("res://pax_keys.gd")

## The player clicked a province (never emitted for sea).
signal province_clicked(province: int)

const MIN_ZOOM := 0.25
const MAX_ZOOM := 16.0

var _canvas := Node2D.new()
var _sprite := Sprite2D.new()
var _material := ShaderMaterial.new()
var _labels := Node2D.new()
var _ids: Image
var _lut: ImageTexture
var _message := Label.new()
var _dragging := false


func _init() -> void:
	clip_contents = true
	mouse_filter = Control.MOUSE_FILTER_STOP
	_sprite.centered = false
	_sprite.texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST
	_material.shader = preload("res://ui/map.gdshader")
	_sprite.material = _material
	_canvas.add_child(_sprite)
	_canvas.add_child(_labels)
	add_child(_canvas)
	_message.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	add_child(_message)
	resized.connect(fit)


## Shows `message` instead of a map (no map, or a map that doesn't match the server's).
func show_message(message: String) -> void:
	_message.text = message
	_canvas.visible = false


## `map`: `PaxClient.load_map`'s result. `provinces`: the names to label.
func set_map(map: Dictionary, provinces: PackedStringArray) -> void:
	var size := Vector2i(map[PaxKeys.WIDTH], map[PaxKeys.HEIGHT])
	_ids = Image.create_from_data(size.x, size.y, false, Image.FORMAT_RGB8, map[PaxKeys.IDS])
	_sprite.texture = ImageTexture.create_from_image(_ids)
	for child in _labels.get_children():
		child.queue_free()
	var anchors: Array = map[PaxKeys.LABELS]
	for p in anchors.size():
		var l := Label.new()
		l.text = provinces[p].capitalize()
		l.add_theme_color_override("font_color", Color.WHITE)
		l.add_theme_color_override("font_outline_color", Color.BLACK)
		l.add_theme_constant_override("outline_size", 4)
		l.position = Vector2(anchors[p])
		_labels.add_child(l)
	_message.text = ""
	_canvas.visible = true
	fit()


## One colour per province, in province order.
func set_colors(colors: PackedColorArray) -> void:
	var lut := Image.create(maxi(colors.size(), 1), 1, false, Image.FORMAT_RGBA8)
	for i in colors.size():
		lut.set_pixel(i, 0, colors[i])
	if _lut == null or _lut.get_width() != lut.get_width():
		_lut = ImageTexture.create_from_image(lut)
		_material.set_shader_parameter("lut", _lut)
	else:
		_lut.update(lut)


## Highlights `province` (or nothing when `null`).
func set_selected(province: Variant) -> void:
	_material.set_shader_parameter("selected_id", 0 if province == null else int(province) + 1)


## Scales the map to fit, centred.
func fit() -> void:
	if _ids == null or size.x <= 0 or size.y <= 0:
		return
	var s := minf(size.x / _ids.get_width(), size.y / _ids.get_height())
	_canvas.scale = Vector2(s, s)
	_canvas.position = (size - Vector2(_ids.get_size()) * s) / 2.0
	_keep_labels_readable()


func _gui_input(event: InputEvent) -> void:
	if _ids == null:
		return
	if event is InputEventMouseButton and event.pressed:
		match event.button_index:
			MOUSE_BUTTON_WHEEL_UP:
				_zoom_at(event.position, 1.25)
			MOUSE_BUTTON_WHEEL_DOWN:
				_zoom_at(event.position, 0.8)
			MOUSE_BUTTON_LEFT:
				var province := _province_at(event.position)
				if province >= 0:
					province_clicked.emit(province)
	if event is InputEventMouseButton and event.button_index in [MOUSE_BUTTON_RIGHT, MOUSE_BUTTON_MIDDLE]:
		_dragging = event.pressed
	if event is InputEventMouseMotion and _dragging:
		_canvas.position += event.relative


func _zoom_at(at: Vector2, factor: float) -> void:
	var s := clampf(_canvas.scale.x * factor, MIN_ZOOM, MAX_ZOOM)
	var map_point := (at - _canvas.position) / _canvas.scale.x
	_canvas.scale = Vector2(s, s)
	_canvas.position = at - map_point * s
	_keep_labels_readable()


## Names stay the same size on screen at any zoom: each label is counter-scaled
## (its position stays in map pixels).
func _keep_labels_readable() -> void:
	for l: Label in _labels.get_children():
		l.scale = Vector2.ONE / _canvas.scale.x


## The province under a point of this control, or -1 for sea or off the map.
func _province_at(at: Vector2) -> int:
	var p := Vector2i(((at - _canvas.position) / _canvas.scale.x).floor())
	if p.x < 0 or p.y < 0 or p.x >= _ids.get_width() or p.y >= _ids.get_height():
		return -1
	var c := _ids.get_pixelv(p)
	return roundi(c.r * 255.0) + 256 * roundi(c.g * 255.0) - 1
