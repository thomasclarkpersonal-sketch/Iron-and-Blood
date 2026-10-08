## The start screen (M4-9): play alone, host a multiplayer game, or join one.
extends CenterContainer

## Single player: the client launches its own server (NETWORK_PROTOCOL §6).
signal single_player_requested
## Host a game for `players` (D24): a server on this machine, reachable over TLS.
signal host_requested(players: int, name: String)
## Join a game: the host's address, port and certificate fingerprint (M4-6), and the
## server's password if it has one.
signal join_requested(host: String, port: int, fingerprint: String, password: String, name: String)

var _scenario: Label
var _name: LineEdit
var _players: SpinBox
var _address: LineEdit
var _port: SpinBox
var _fingerprint: LineEdit
var _password: LineEdit
var _status: Label


func _init() -> void:
	set_anchors_preset(Control.PRESET_FULL_RECT)
	var column := VBoxContainer.new()
	column.custom_minimum_size.x = 480
	column.add_theme_constant_override("separation", 10)
	var title := Label.new()
	title.text = "Iron and Blood"
	title.add_theme_font_size_override("font_size", 32)
	column.add_child(title)
	_scenario = Label.new()
	column.add_child(_scenario)

	_name = _field(column, "Your name", "Player")
	var single := Button.new()
	single.text = "Single player (sandbox)"
	single.pressed.connect(func() -> void: single_player_requested.emit())
	column.add_child(single)

	column.add_child(HSeparator.new())
	var host_row := HBoxContainer.new()
	var host_label := Label.new()
	host_label.text = "Players"
	host_row.add_child(host_label)
	_players = SpinBox.new()
	_players.min_value = 2
	_players.max_value = 8
	_players.value = 2
	host_row.add_child(_players)
	var host := Button.new()
	host.text = "Host a game"
	host.pressed.connect(func() -> void: host_requested.emit(int(_players.value), _name.text.strip_edges()))
	host_row.add_child(host)
	column.add_child(host_row)

	column.add_child(HSeparator.new())
	_address = _field(column, "Host address", "127.0.0.1")
	var port_row := HBoxContainer.new()
	var port_label := Label.new()
	port_label.text = "Port"
	port_label.custom_minimum_size.x = 130
	port_row.add_child(port_label)
	_port = SpinBox.new()
	_port.min_value = 1
	_port.max_value = 65535
	_port.value = 7777
	port_row.add_child(_port)
	column.add_child(port_row)
	_fingerprint = _field(column, "Fingerprint", "")
	_fingerprint.placeholder_text = "the 64 hex digits the host shared"
	_password = _field(column, "Password", "")
	_password.secret = true
	_password.placeholder_text = "if the server has one"
	var join := Button.new()
	join.text = "Join a game"
	join.pressed.connect(func() -> void:
		join_requested.emit(_address.text.strip_edges(), int(_port.value), _fingerprint.text.strip_edges(),
			_password.text, _name.text.strip_edges()))
	column.add_child(join)

	_status = Label.new()
	_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(_status)
	add_child(column)


func set_scenario(name: String) -> void:
	_scenario.text = "Scenario: %s" % name


## Something the player should know: why hosting or joining failed.
func show_status(text: String) -> void:
	_status.text = text


func _field(column: VBoxContainer, label: String, value: String) -> LineEdit:
	var row := HBoxContainer.new()
	var l := Label.new()
	l.text = label
	l.custom_minimum_size.x = 130
	row.add_child(l)
	var edit := LineEdit.new()
	edit.text = value
	edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(edit)
	column.add_child(row)
	return edit
