## The lobby (D24, M4-2, M4-9): who is in the game, which nation each holds, who is
## ready, and, for the host, the start and kick buttons. Everything shown comes from
## the server's `LobbyState`; the buttons only ask (`PaxClient.claim_nation`, …).
extends ColorRect

const PaxKeys := preload("res://pax_keys.gd")

signal claim_requested(nation: Variant)
signal ready_requested(ready: bool)
signal start_requested
signal kick_requested(player: int)

var _share: Label
var _rows: VBoxContainer
var _nation: OptionButton
var _ready: CheckButton
var _start: Button
var _notice: Label
var _nations: PackedStringArray = []
## This client's player id (from its Welcome).
var _me := -1


func _init() -> void:
	color = Color(0.06, 0.07, 0.09, 0.94)
	set_anchors_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_STOP
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	var panel := PanelContainer.new()
	var column := VBoxContainer.new()
	column.custom_minimum_size.x = 620
	column.add_theme_constant_override("separation", 10)
	var title := Label.new()
	title.text = "Lobby"
	title.add_theme_font_size_override("font_size", 24)
	column.add_child(title)
	_share = Label.new()
	_share.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_share.visible = false
	column.add_child(_share)
	_rows = VBoxContainer.new()
	column.add_child(_rows)
	var choose := HBoxContainer.new()
	var label := Label.new()
	label.text = "Your nation"
	choose.add_child(label)
	_nation = OptionButton.new()
	_nation.item_selected.connect(func(i: int) -> void:
		claim_requested.emit(null if i == 0 else i - 1))
	choose.add_child(_nation)
	_ready = CheckButton.new()
	_ready.text = "Ready"
	_ready.toggled.connect(func(on: bool) -> void: ready_requested.emit(on))
	choose.add_child(_ready)
	column.add_child(choose)
	_start = Button.new()
	_start.text = "Start the game"
	_start.pressed.connect(func() -> void: start_requested.emit())
	column.add_child(_start)
	_notice = Label.new()
	_notice.add_theme_color_override("font_color", Color(1.0, 0.75, 0.4))
	_notice.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(_notice)
	panel.add_child(column)
	center.add_child(panel)
	add_child(center)
	visible = false


## A new session: the scenario's nations, and who this client is.
func set_session(welcome: Dictionary) -> void:
	_me = welcome[PaxKeys.PLAYER]
	_nations = welcome[PaxKeys.NATIONS]
	_nation.clear()
	_nation.add_item("(none)")
	for n in _nations:
		_nation.add_item(n.capitalize())
	_notice.text = ""


## What the host shares with the players (`PaxClient.hosted()`), shown only to them.
func set_hosting(hosted: Dictionary) -> void:
	_share.visible = not hosted.is_empty()
	if not hosted.is_empty():
		_share.text = "Players join with port %d and fingerprint\n%s" % [
			hosted[PaxKeys.PORT], hosted[PaxKeys.FINGERPRINT]]


## A `LOBBY_STATE` event: the players, and why this client's last request was
## refused, if it was.
func show_lobby(event: Dictionary) -> void:
	for child in _rows.get_children():
		_rows.remove_child(child)
		child.queue_free()
	var t: Dictionary = event[PaxKeys.LOBBY_PLAYERS]
	var players: PackedInt64Array = t[PaxKeys.PLAYER]
	var i_am_host := false
	for row in players.size():
		var player: int = players[row]
		var nation = t[PaxKeys.NATION][row]
		var mine := player == _me
		if mine:
			i_am_host = t[PaxKeys.HOST][row]
			_nation.select(0 if nation == null else nation + 1)
			_ready.set_pressed_no_signal(t[PaxKeys.READY][row])
		var line := HBoxContainer.new()
		var who := Label.new()
		var playing := "sandbox" if t[PaxKeys.SANDBOX][row] else ("—" if nation == null else _nations[nation].capitalize())
		var marks := PackedStringArray()
		if t[PaxKeys.HOST][row]:
			marks.append("host")
		if t[PaxKeys.READY][row]:
			marks.append("ready")
		if t[PaxKeys.AWAY][row]:
			marks.append("away")
		if mine:
			marks.append("you")
		who.text = "%d  %s · %s  %s" % [player, t[PaxKeys.NAME][row], playing,
			"(%s)" % ", ".join(marks) if not marks.is_empty() else ""]
		who.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		line.add_child(who)
		line.set_meta("player", player)
		_rows.add_child(line)
	# The host's controls, for the host only.
	for line in _rows.get_children():
		var player: int = line.get_meta("player")
		if i_am_host and player != _me:
			var kick := Button.new()
			kick.text = "Kick"
			kick.pressed.connect(func() -> void: kick_requested.emit(player))
			line.add_child(kick)
	_start.visible = i_am_host
	var notice = event[PaxKeys.NOTICE]
	if notice != null:
		_notice.text = notice
	visible = not event[PaxKeys.STARTED]


## Whether this client is the host, from the latest lobby.
static func is_host(event: Dictionary, me: int) -> bool:
	var t: Dictionary = event[PaxKeys.LOBBY_PLAYERS]
	var players: PackedInt64Array = t[PaxKeys.PLAYER]
	for row in players.size():
		if players[row] == me:
			return t[PaxKeys.HOST][row]
	return false


## This client's row of the latest lobby as a Dictionary (`NATION`, `SANDBOX`, …), or
## empty if it isn't there.
static func me(event: Dictionary, player: int) -> Dictionary:
	if event.is_empty():
		return {}
	var t: Dictionary = event[PaxKeys.LOBBY_PLAYERS]
	var row := (t[PaxKeys.PLAYER] as PackedInt64Array).find(player)
	if row < 0:
		return {}
	var mine := {}
	for key in t:
		mine[key] = t[key][row]
	return mine


## The names of `players` (ids), from the latest lobby: who a fairness pause waits for.
static func names_of(event: Dictionary, players: PackedInt64Array) -> PackedStringArray:
	var names := PackedStringArray()
	if event.is_empty():
		return names
	var t: Dictionary = event[PaxKeys.LOBBY_PLAYERS]
	var ids: PackedInt64Array = t[PaxKeys.PLAYER]
	for p in players:
		var row := ids.find(p)
		names.append(t[PaxKeys.NAME][row] if row >= 0 else "player %d" % p)
	return names
