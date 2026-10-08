## The connection-lost screen (M3-8d): the reason the session ended (a `Goodbye`, a
## protocol error, a refusal or a closed socket), and what the player can do about
## it: rejoin a multiplayer game with the resume token (M4-9), start a new game, or
## quit.
extends ColorRect

signal restart_requested
signal rejoin_requested

var _reason: Label
var _rejoin: Button


func _init() -> void:
	color = Color(0, 0, 0, 0.72)
	set_anchors_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_STOP
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	var panel := PanelContainer.new()
	var column := VBoxContainer.new()
	column.custom_minimum_size.x = 460
	var title := Label.new()
	title.text = "Connection lost"
	title.add_theme_font_size_override("font_size", 24)
	column.add_child(title)
	_reason = Label.new()
	_reason.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(_reason)
	var buttons := HBoxContainer.new()
	_rejoin = Button.new()
	_rejoin.text = "Rejoin"
	_rejoin.visible = false
	_rejoin.pressed.connect(func() -> void: rejoin_requested.emit())
	buttons.add_child(_rejoin)
	var restart := Button.new()
	restart.text = "Start a new game"
	restart.pressed.connect(func() -> void: restart_requested.emit())
	buttons.add_child(restart)
	var quit := Button.new()
	quit.text = "Quit"
	quit.pressed.connect(func() -> void: get_tree().quit())
	buttons.add_child(quit)
	column.add_child(buttons)
	panel.add_child(column)
	center.add_child(panel)
	add_child(center)
	visible = false


## `can_rejoin`: a multiplayer game whose seat waits for this client's token (D24).
func show_reason(reason: String, can_rejoin := false) -> void:
	_reason.text = reason
	_rejoin.visible = can_rejoin
	visible = true
