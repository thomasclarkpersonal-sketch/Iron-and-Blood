## The save/load menu (M3-8d, D23): the server's saves (`ListSaves`), save under a
## name (`SaveGame`), load one (`LoadGame`). The server answers each: `SaveList`,
## `SaveResult`, or for a successful load a new `Welcome` (main.gd handles that).
extends PanelContainer

const PaxKeys := preload("res://pax_keys.gd")

signal save_requested(name: String)
signal load_requested(name: String)
signal refresh_requested

var _list: ItemList
var _name: LineEdit
var _status: Label


func _init() -> void:
	var style := StyleBoxFlat.new()
	style.bg_color = Color(0.13, 0.14, 0.17)
	style.set_content_margin_all(12)
	style.set_corner_radius_all(4)
	add_theme_stylebox_override("panel", style)
	var column := VBoxContainer.new()
	column.custom_minimum_size = Vector2(380, 340)
	var title := Label.new()
	title.text = "Saved games"
	title.add_theme_font_size_override("font_size", 20)
	column.add_child(title)
	_list = ItemList.new()
	_list.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_list.item_selected.connect(func(i: int) -> void: _name.text = _list.get_item_text(i))
	_list.item_activated.connect(func(i: int) -> void: load_requested.emit(_list.get_item_text(i)))
	column.add_child(_list)
	_name = LineEdit.new()
	_name.placeholder_text = "Name: letters, digits, - and _"
	_name.max_length = 64
	column.add_child(_name)
	var buttons := HBoxContainer.new()
	for entry in [["Save", func() -> void: save_requested.emit(_name.text.strip_edges())],
			["Load", func() -> void: load_requested.emit(_name.text.strip_edges())],
			["Close", func() -> void: visible = false]]:
		var b := Button.new()
		b.text = entry[0]
		b.pressed.connect(entry[1])
		buttons.add_child(b)
	column.add_child(buttons)
	_status = Label.new()
	_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(_status)
	add_child(column)
	visible = false


## Opens the menu and asks the server for its saves.
func open() -> void:
	visible = true
	_status.text = ""
	refresh_requested.emit()


func show_list(names: PackedStringArray) -> void:
	_list.clear()
	for n in names:
		_list.add_item(n)


func show_result(result: Dictionary) -> void:
	var error: String = result[PaxKeys.ERROR]
	var saving: bool = result[PaxKeys.REQUEST] == PaxKeys.REQUEST_SAVE
	if error == "":
		# Only a save succeeds with a SaveResult; a load succeeds with a Welcome.
		_status.text = "Saved as %s." % result[PaxKeys.NAME]
		refresh_requested.emit()
	else:
		_status.text = "%s failed: %s" % ["Save" if saving else "Load", error]


func show_status(text: String) -> void:
	_status.text = text
