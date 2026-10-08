## The top bar: the date, the speed controls (D23) and who is playing.
extends PanelContainer

const PaxKeys := preload("res://pax_keys.gd")

## The player asked for `speed` (0 pauses, 1 to 5 from slowest to fastest).
signal speed_requested(speed: int)

const SPEED_LABELS := ["Pause", "0.5", "1", "2", "5", "Max"]
const SPEED_TIPS := ["Paused", "0.5 days per second", "1 day per second", "2 days per second",
	"5 days per second", "As fast as the server can"]

var _day: Label
var _session: Label
var _buttons: Array[Button] = []


func _init() -> void:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 12)
	_day = Label.new()
	_day.text = "Connecting…"
	_day.custom_minimum_size.x = 120
	row.add_child(_day)
	var group := ButtonGroup.new()
	for speed in SPEED_LABELS.size():
		var b := Button.new()
		b.text = SPEED_LABELS[speed]
		b.tooltip_text = SPEED_TIPS[speed]
		b.toggle_mode = true
		b.button_group = group
		b.pressed.connect(func() -> void: speed_requested.emit(speed))
		_buttons.append(b)
		row.add_child(b)
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(spacer)
	_session = Label.new()
	row.add_child(_session)
	add_child(row)


func set_session(welcome: Dictionary) -> void:
	var nation = welcome[PaxKeys.NATION]
	var playing := "sandbox" if nation == null else (welcome[PaxKeys.NATIONS] as PackedStringArray)[nation].capitalize()
	_session.text = "%s · %s" % [welcome[PaxKeys.SCENARIO], playing]
	show_speed(welcome[PaxKeys.SPEED])
	_day.text = "Day %d" % welcome[PaxKeys.DAY]


func show_day(update: Dictionary) -> void:
	_day.text = "Day %d" % update[PaxKeys.DAY]


func show_speed(speed: int) -> void:
	if speed >= 0 and speed < _buttons.size():
		_buttons[speed].set_pressed_no_signal(true)
