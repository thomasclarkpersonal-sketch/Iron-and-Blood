## The top bar: the date, the speed controls (D23), who is playing, and who a
## multiplayer game is waiting for (D24).
extends PanelContainer

const PaxKeys := preload("res://pax_keys.gd")

## The player asked for `speed` (a `PaxKeys.SPEED_*`).
signal speed_requested(speed: int)
## The player opened the save/load menu.
signal saves_requested

## The speeds, in button order (D23). Labels and tooltips come from the server's
## pacing table (`PaxKeys.SPEED_DAY_MS`), so they can't disagree with it.
const SPEEDS := [PaxKeys.SPEED_PAUSED, PaxKeys.SPEED_SLOWEST, PaxKeys.SPEED_SLOW, PaxKeys.SPEED_NORMAL,
	PaxKeys.SPEED_FAST, PaxKeys.SPEED_FASTEST]

var _day: Label
var _session: Label
var _scenario := ""
var _nations: PackedStringArray = []
var _waiting: Label
var _buttons: Array[Button] = []


func _init() -> void:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 12)
	_day = Label.new()
	_day.text = "Connecting…"
	_day.custom_minimum_size.x = 120
	row.add_child(_day)
	var group := ButtonGroup.new()
	for speed: int in SPEEDS:
		var b := Button.new()
		var ms = PaxKeys.SPEED_DAY_MS[speed]
		if ms == null:
			b.text = "Pause"
			b.tooltip_text = "Paused"
		elif ms == 0:
			b.text = "Max"
			b.tooltip_text = "As fast as the server can"
		else:
			var per_second: float = 1000.0 / ms
			b.text = _number(per_second)
			b.tooltip_text = "%s days per second" % _number(per_second)
		b.toggle_mode = true
		b.button_group = group
		b.set_meta("speed", speed)
		b.pressed.connect(func() -> void: speed_requested.emit(speed))
		_buttons.append(b)
		row.add_child(b)
	_waiting = Label.new()
	_waiting.add_theme_color_override("font_color", Color(1.0, 0.75, 0.4))
	row.add_child(_waiting)
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(spacer)
	_session = Label.new()
	row.add_child(_session)
	var saves := Button.new()
	saves.text = "Saves"
	saves.pressed.connect(func() -> void: saves_requested.emit())
	row.add_child(saves)
	add_child(row)


func set_session(welcome: Dictionary) -> void:
	_scenario = welcome[PaxKeys.SCENARIO]
	_nations = welcome[PaxKeys.NATIONS]
	set_playing(welcome[PaxKeys.NATION], welcome[PaxKeys.NATION] == null)
	show_speed(welcome[PaxKeys.SPEED])
	_day.text = "Day %d" % welcome[PaxKeys.DAY]


func show_day(update: Dictionary) -> void:
	_day.text = "Day %d" % update[PaxKeys.DAY]


## Who this player plays: a nation, sandbox, or (in a lobby) none yet.
func set_playing(nation: Variant, sandbox: bool) -> void:
	var playing := "sandbox" if sandbox else ("no nation yet" if nation == null else _nations[nation].capitalize())
	_session.text = "%s · %s" % [_scenario, playing]


## The players a fairness pause waits for (D24); empty when none.
func show_waiting(names: PackedStringArray) -> void:
	_waiting.text = "" if names.is_empty() else "Waiting for %s" % ", ".join(names)


## Only the host sets the speed in a multiplayer game; anyone may pause (D24).
func set_host(host: bool) -> void:
	for b in _buttons:
		b.disabled = not host and b.get_meta("speed") != PaxKeys.SPEED_PAUSED


func show_speed(speed: int) -> void:
	for b in _buttons:
		b.set_pressed_no_signal(b.get_meta("speed") == speed)


## A number without a trailing ".0" for whole values (display only).
static func _number(x: float) -> String:
	return str(int(x)) if is_equal_approx(x, roundf(x)) else str(x)
