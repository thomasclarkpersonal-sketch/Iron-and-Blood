## The map-mode bar: which `MapMode` the map shows (D22), the good for `Price`, and
## the legend.
extends HBoxContainer

const PaxKeys := preload("res://pax_keys.gd")

signal mode_changed(mode: int, good: int)

const MODES := [
	["Nations", PaxKeys.MAP_MODE_NATION],
	["Population", PaxKeys.MAP_MODE_POPULATION],
	["Unemployment", PaxKeys.MAP_MODE_UNEMPLOYMENT],
	["Life needs", PaxKeys.MAP_MODE_LIFE_NEEDS],
	["Militancy", PaxKeys.MAP_MODE_MILITANCY],
	["Price", PaxKeys.MAP_MODE_PRICE],
]

var mode: int = PaxKeys.MAP_MODE_NATION
var good := 0
var _goods: OptionButton
var _buttons: Array[Button] = []
var _legend: Label


func _init() -> void:
	var group := ButtonGroup.new()
	for entry in MODES:
		var b := Button.new()
		b.text = entry[0]
		b.toggle_mode = true
		b.button_group = group
		b.button_pressed = entry[1] == mode
		var value: int = entry[1]
		b.set_meta("mode", value)
		b.pressed.connect(func() -> void: _select(value))
		_buttons.append(b)
		add_child(b)
	_goods = OptionButton.new()
	_goods.visible = false
	_goods.item_selected.connect(func(i: int) -> void:
		good = i
		mode_changed.emit(mode, good))
	add_child(_goods)
	_legend = Label.new()
	_legend.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_legend.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	# A long legend shortens rather than widening the map column.
	_legend.clip_text = true
	_legend.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	add_child(_legend)


func set_goods(goods: PackedStringArray) -> void:
	_goods.clear()
	for g in goods:
		_goods.add_item(g.capitalize())
	good = mini(good, maxi(goods.size() - 1, 0))


func set_legend(text: String) -> void:
	_legend.text = text


## Switches to `value` (a `PaxKeys.MAP_MODE_*`), as if its button were pressed.
func select(value: int) -> void:
	for b: Button in _buttons:
		b.set_pressed_no_signal(b.get_meta("mode") == value)
	_select(value)


func _select(value: int) -> void:
	mode = value
	_goods.visible = mode == PaxKeys.MAP_MODE_PRICE
	mode_changed.emit(mode, good)
