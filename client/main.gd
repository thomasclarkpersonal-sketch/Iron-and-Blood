## The Iron and Blood client (M3-8, D12): launches the local server, opens a
## session and shows what the server sends. It never simulates.
##
## Everything about the protocol (the connection, decoding, acknowledgements,
## keep-alive) is in the Rust bridge, `PaxClient`. This script only routes events
## to the UI.
##
## Command-line options (after `--`):
##   --autostart            skip the start screen
##   --scenario=DIR         scenario directory (default: ../scenarios/two_states)
##   --server=PATH          pax_server binary (default: ../target/debug/pax_server)
##   --nation=N             play nation N (default: sandbox)
##   --screenshot=PATH      run to day 30, pause, save a screenshot and quit
##   --smoke                headless check: run to day 40, print SMOKE OK and quit
extends Control

const PaxKeys := preload("res://pax_keys.gd")
const TopBar := preload("res://ui/top_bar.gd")
const SummaryPanel := preload("res://ui/summary_panel.gd")
const DebugOverlay := preload("res://ui/debug_overlay.gd")
const Format := preload("res://ui/format.gd")

const SMOKE_DAYS := 40
const SCREENSHOT_DAYS := 30
const SMOKE_TIMEOUT_MS := 60_000

var client: PaxClient
var welcome: Dictionary = {}
var last_update: Dictionary = {}

var start_screen: Control
var game: Control
var top_bar: TopBar
var summary: SummaryPanel
var overlay: DebugOverlay
var lost: Label

var _started_ms := 0
var _finishing := false


func _ready() -> void:
	_build_ui()
	if _flag("--autostart") or _flag("--smoke") or _arg("--screenshot=") != "":
		_start()


func _process(_delta: float) -> void:
	if client == null or _finishing:
		return
	for event: Dictionary in client.poll():
		if not _finishing:
			_handle(event)
	if _flag("--smoke") and Time.get_ticks_msec() - _started_ms > SMOKE_TIMEOUT_MS:
		_finish(1, "SMOKE FAILED: no day %d within %d s" % [SMOKE_DAYS, SMOKE_TIMEOUT_MS / 1000])


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and event.keycode == KEY_F3:
		overlay.visible = not overlay.visible


func _start() -> void:
	start_screen.visible = false
	game.visible = true
	_started_ms = Time.get_ticks_msec()
	client = PaxClient.new()
	var error := client.launch(_server_path(), _scenario_dir(), OS.get_user_data_dir().path_join("saves"))
	if error != "":
		_connection_lost(error)
		return
	var nation := _arg("--nation=")
	error = client.hello(int(nation) if nation.is_valid_int() else null)
	if error != "":
		_connection_lost(error)


func _handle(event: Dictionary) -> void:
	match event[PaxKeys.TYPE]:
		PaxKeys.WELCOME:
			welcome = event
			top_bar.set_session(welcome)
			summary.set_session(welcome)
			client.subscribe(PaxKeys.MAP_MODE_NATION, 0, null, null) # the map arrives with M3-8b
			if _flag("--smoke") or _arg("--screenshot=") != "":
				client.set_speed(PaxKeys.SPEED_FASTEST)
		PaxKeys.DAY_UPDATE:
			last_update = event
			top_bar.show_day(event)
			summary.show_update(event)
			overlay.show_update(event)
			_check_scripted_runs(event[PaxKeys.DAY])
		PaxKeys.SERVER_STATE:
			top_bar.show_speed(event[PaxKeys.SPEED])
		PaxKeys.CLOSED:
			_connection_lost(event[PaxKeys.REASON])


## The --smoke and --screenshot runs: stop once the game has run long enough.
func _check_scripted_runs(day: int) -> void:
	if _flag("--smoke") and day >= SMOKE_DAYS:
		print("SMOKE OK: day %d, state %s, %s" % [day, Format.hash_hex(last_update[PaxKeys.STATE_HASH]), welcome[PaxKeys.SCENARIO]])
		_finish(0, "")
	var shot := _arg("--screenshot=")
	if shot != "" and day >= SCREENSHOT_DAYS and not _finishing:
		_finishing = true
		client.set_speed(PaxKeys.SPEED_PAUSED)
		await get_tree().create_timer(0.3).timeout
		await RenderingServer.frame_post_draw
		get_viewport().get_texture().get_image().save_png(shot)
		print("SCREENSHOT %s" % shot)
		_finish(0, "")


func _connection_lost(reason: String) -> void:
	lost.text = "Connection lost: %s" % reason
	lost.visible = true
	if _flag("--smoke") or _arg("--screenshot=") != "":
		_finish(1, "FAILED: connection lost: %s" % reason)


func _finish(code: int, message: String) -> void:
	_finishing = true
	if message != "":
		printerr(message)
	if client != null:
		client.disconnect_from_server()
	get_tree().quit(code)


func _exit_tree() -> void:
	if client != null:
		client.disconnect_from_server()


func _build_ui() -> void:
	set_anchors_preset(Control.PRESET_FULL_RECT)

	start_screen = VBoxContainer.new()
	start_screen.set_anchors_preset(Control.PRESET_CENTER)
	var title := Label.new()
	title.text = "Iron and Blood"
	title.add_theme_font_size_override("font_size", 32)
	start_screen.add_child(title)
	var scenario := Label.new()
	scenario.text = "Scenario: %s" % _scenario_dir().get_file()
	start_screen.add_child(scenario)
	var start := Button.new()
	start.text = "Start (sandbox)"
	start.pressed.connect(_start)
	start_screen.add_child(start)
	add_child(start_screen)

	game = VBoxContainer.new()
	game.set_anchors_preset(Control.PRESET_FULL_RECT)
	game.visible = false
	top_bar = TopBar.new()
	top_bar.speed_requested.connect(func(speed: int) -> void: client.set_speed(speed))
	game.add_child(top_bar)
	summary = SummaryPanel.new()
	summary.size_flags_vertical = Control.SIZE_EXPAND_FILL
	game.add_child(summary)
	add_child(game)

	overlay = DebugOverlay.new()
	add_child(overlay)
	overlay.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_LEFT, Control.PRESET_MODE_MINSIZE, 8)
	overlay.grow_vertical = Control.GROW_DIRECTION_BEGIN
	# Hidden until F3, except in screenshots, which show what a bug report needs.
	overlay.visible = _arg("--screenshot=") != ""

	lost = Label.new()
	lost.set_anchors_preset(Control.PRESET_CENTER)
	lost.add_theme_color_override("font_color", Color(1, 0.4, 0.4))
	lost.visible = false
	add_child(lost)


func _project_dir() -> String:
	return ProjectSettings.globalize_path("res://")


func _scenario_dir() -> String:
	var dir := _arg("--scenario=")
	return dir if dir != "" else _project_dir().path_join("../scenarios/two_states").simplify_path()


func _server_path() -> String:
	var path := _arg("--server=")
	if path != "":
		return path
	if OS.has_environment("PAX_SERVER"):
		return OS.get_environment("PAX_SERVER")
	var exe := ".exe" if OS.get_name() == "Windows" else ""
	return _project_dir().path_join("../target/debug/pax_server" + exe).simplify_path()


func _flag(name: String) -> bool:
	return name in OS.get_cmdline_user_args()


func _arg(prefix: String) -> String:
	for a in OS.get_cmdline_user_args():
		if a.begins_with(prefix):
			return a.substr(prefix.length())
	return ""
