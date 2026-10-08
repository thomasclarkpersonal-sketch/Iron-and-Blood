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
##   --map-mode=N           start in map mode N (a PaxKeys.MAP_MODE_* value)
##   --tab=NAME             start on a side-panel tab: World, Nation, Market or Province
##   --select=N             select province N at start
##   --open-saves           open the save/load menu at start
##   --screenshot=PATH      run to day 30, pause, save a screenshot and quit
##   --smoke                headless check: select a province, switch to the Price map, set a policy,
##                          save, list and load at day 20, run to day 40, check the views, the
##                          policy and the reload, print SMOKE OK, quit
extends Control

const PaxKeys := preload("res://pax_keys.gd")
const TopBar := preload("res://ui/top_bar.gd")
const SummaryPanel := preload("res://ui/summary_panel.gd")
const DebugOverlay := preload("res://ui/debug_overlay.gd")
const Format := preload("res://ui/format.gd")
const MapView := preload("res://ui/map_view.gd")
const MapModes := preload("res://ui/map_modes.gd")
const MapColors := preload("res://ui/map_colors.gd")
const NationPanel := preload("res://ui/nation_panel.gd")
const MarketPanel := preload("res://ui/market_panel.gd")
const ProvincePanel := preload("res://ui/province_panel.gd")
const SaveMenu := preload("res://ui/save_menu.gd")
const LostScreen := preload("res://ui/lost_screen.gd")

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
var tabs: TabContainer
var nation_panel: NationPanel
var market_panel: MarketPanel
var province_panel: ProvincePanel
var map_view: MapView
var map_modes: MapModes
var province_info: Label
var overlay: DebugOverlay
var save_menu: SaveMenu
var lost: LostScreen

## Whether this connection has had its first Welcome (a later one is a load).
var _session_started := false
## The smoke test's save-and-load round trip: 0 not begun, 1 saving, 2 listing,
## 3 loading, 4 reloaded.
var _smoke_stage := 0
const SMOKE_SAVE := "smoke"
const SMOKE_SAVE_DAY := 20
## The day the smoke test's load returned to: the final check needs days after it.
var _smoke_reload_day := 0
## The policy the smoke test sets (income tax of nation 0), in per mille.
const SMOKE_TAX_PER_MILLE := 123
## The smoke test's command, once accepted: its `client_seq`.
var _smoke_command: Variant = null
## The province panel's province, or `null` for none (D22: no sentinels).
var selected_province: Variant = null
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
	lost.visible = false
	_started_ms = Time.get_ticks_msec()
	_session_started = false
	welcome = {}
	last_update = {}
	selected_province = null
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
			var reload := _session_started
			_session_started = true
			welcome = event
			if reload:
				# A load replaced the game (D23): its tables may differ, so start afresh.
				selected_province = null
				map_view.set_selected(null)
				save_menu.visible = false
			top_bar.set_session(welcome)
			summary.set_session(welcome)
			nation_panel.set_session(welcome)
			market_panel.set_session(welcome)
			province_panel.set_session(welcome)
			_load_map()
			_subscribe()
			var select := _arg("--select=")
			if select.is_valid_int():
				_select_province(int(select))
			if reload and _flag("--smoke"):
				_smoke_stage = 4
				_smoke_reload_day = welcome[PaxKeys.DAY]
				_select_province(0)
				map_modes.select(PaxKeys.MAP_MODE_PRICE)
			elif _flag("--smoke"):
				# Exercise the subscription path: a province panel and a value map mode.
				_select_province(0)
				map_modes.select(PaxKeys.MAP_MODE_PRICE)
				_smoke_command = client.submit_policy(PaxKeys.POLICY_INCOME_TAX, 0,
					PaxClient.rate_from_per_mille(SMOKE_TAX_PER_MILLE))
			if _flag("--open-saves") and not reload:
				save_menu.open()
			if _flag("--smoke") or _arg("--screenshot=") != "":
				client.set_speed(PaxKeys.SPEED_FASTEST)
		PaxKeys.DAY_UPDATE:
			last_update = event
			top_bar.show_day(event)
			summary.show_update(event)
			nation_panel.show_update(event)
			market_panel.show_update(event)
			province_panel.show_update(event)
			overlay.show_update(event)
			_show_map(event)
			_check_scripted_runs(event[PaxKeys.DAY])
		PaxKeys.COMMAND_RESULT:
			nation_panel.show_result(event)
			if event[PaxKeys.CLIENT_SEQ] == _smoke_command and event[PaxKeys.COMMAND_ERROR] != PaxKeys.COMMAND_ERROR_NONE:
				_finish(1, "SMOKE FAILED: the policy command was refused: %s" % event)
		PaxKeys.SERVER_STATE:
			top_bar.show_speed(event[PaxKeys.SPEED])
		PaxKeys.SAVE_LIST:
			save_menu.show_list(event[PaxKeys.NAMES])
			if _smoke_stage == 2 and SMOKE_SAVE in event[PaxKeys.NAMES]:
				_smoke_stage = 3
				client.load_game(SMOKE_SAVE)
		PaxKeys.SAVE_RESULT:
			save_menu.show_result(event)
			if _smoke_stage == 1 and event[PaxKeys.NAME] == SMOKE_SAVE:
				if event[PaxKeys.ERROR] != "":
					_finish(1, "SMOKE FAILED: save: %s" % event[PaxKeys.ERROR])
				_smoke_stage = 2
				client.list_saves()
			elif _smoke_stage == 3:
				_finish(1, "SMOKE FAILED: load: %s" % event[PaxKeys.ERROR])
		PaxKeys.CLOSED:
			_connection_lost(event[PaxKeys.REASON])


## The --smoke and --screenshot runs: stop once the game has run long enough.
func _check_scripted_runs(day: int) -> void:
	if _flag("--smoke") and _smoke_stage == 0 and day >= SMOKE_SAVE_DAY:
		_smoke_stage = 1
		client.save_game(SMOKE_SAVE)
	if _flag("--smoke") and _smoke_stage == 4 and day >= maxi(SMOKE_DAYS, _smoke_reload_day + 5):
		var map = last_update[PaxKeys.MAP]
		var province = last_update[PaxKeys.PROVINCE]
		if map == null or map[PaxKeys.MODE] != PaxKeys.MAP_MODE_PRICE or province == null or province[PaxKeys.PROVINCE_ID] != 0:
			_finish(1, "SMOKE FAILED: the update lacks the subscribed views: map %s, province %s" % [map, province])
			return
		var tax: int = last_update[PaxKeys.NATION_TABLE][PaxKeys.INCOME_TAX_RATE_RAW][0]
		if _smoke_command == null or tax != PaxClient.rate_from_per_mille(SMOKE_TAX_PER_MILLE):
			_finish(1, "SMOKE FAILED: the income tax policy didn't apply (rate %d)" % tax)
			return
		print("SMOKE OK: day %d, state %s, %s, price map, province panel, a policy, save and load" % [
			day, Format.hash_hex(last_update[PaxKeys.STATE_HASH]), welcome[PaxKeys.SCENARIO]])
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


## Loads this scenario's province map, checked against the server's (`map_hash`).
func _load_map() -> void:
	map_modes.set_goods(welcome[PaxKeys.GOODS])
	# The bridge knows the session's map (from its Welcome) and the scenario (launch).
	var map: Dictionary = client.load_map()
	if map[PaxKeys.ERROR] != "":
		map_view.show_message("The map can't be shown:\n" + map[PaxKeys.ERROR])
	elif not map.has(PaxKeys.IDS):
		# A protocol 1.0 server sends a map hash but not where the map is.
		var old_server: bool = welcome[PaxKeys.MAP_HASH] != null and welcome[PaxKeys.MAP_DIR] == null
		map_view.show_message("The server is too old to say where its map is (protocol 1.0)." if old_server
			else "This scenario has no map.")
	else:
		map_view.set_map(map, welcome[PaxKeys.PROVINCES])
		map_view.set_colors(MapColors.nations(welcome))


## Asks for the views on screen: the map mode, and the selected province's panel and
## its market's. The answer is an update for the current day (D22).
func _subscribe() -> void:
	if client == null or welcome.is_empty():
		return
	var market: Variant = null
	if selected_province != null:
		market = welcome[PaxKeys.PROVINCE_MARKET][selected_province]
	client.subscribe(map_modes.mode, map_modes.good, market, selected_province)


func _show_map(update: Dictionary) -> void:
	var map = update[PaxKeys.MAP]
	if map == null:
		return
	var mode: int = map[PaxKeys.MODE]
	var values: PackedFloat64Array = map[PaxKeys.VALUES]
	if mode == PaxKeys.MAP_MODE_NATION:
		map_view.set_colors(MapColors.nations(welcome))
	else:
		map_view.set_colors(MapColors.values(mode, values))
	var good: String = welcome[PaxKeys.GOODS][map[PaxKeys.GOOD]] if mode == PaxKeys.MAP_MODE_PRICE else ""
	map_modes.set_legend(MapColors.legend(mode, values, good))
	_show_province(values, mode)


func _show_province(values: PackedFloat64Array, mode: int) -> void:
	if selected_province == null:
		province_info.text = "Click a province to select it. Wheel zooms; right-drag pans."
		return
	var p: int = selected_province
	var market: int = welcome[PaxKeys.PROVINCE_MARKET][p]
	var text := "%s · market %s" % [(welcome[PaxKeys.PROVINCES][p] as String).capitalize(),
		(welcome[PaxKeys.MARKETS][market] as String).capitalize()]
	if p < values.size():
		var v := values[p]
		match mode:
			PaxKeys.MAP_MODE_POPULATION:
				text += " · population %s" % Format.count(int(v))
			PaxKeys.MAP_MODE_PRICE:
				text += " · price %.2f" % v
			_:
				text += " · %s" % Format.percent(v)
	province_info.text = text


func _select_province(province: int) -> void:
	selected_province = province
	map_view.set_selected(province)
	_subscribe()
	# From the world summary, a selection opens the province's own panel.
	if tabs.get_current_tab_control() == summary:
		tabs.current_tab = tabs.get_tab_idx_from_control(tabs.get_node("Province"))


func _connection_lost(reason: String) -> void:
	lost.show_reason(reason)
	save_menu.visible = false
	var shot := _arg("--screenshot=")
	if shot != "" and not _finishing:
		# Show what the player would see, then fail the run.
		_finishing = true
		await RenderingServer.frame_post_draw
		get_viewport().get_texture().get_image().save_png(shot)
		print("SCREENSHOT %s" % shot)
	if _flag("--smoke") or shot != "":
		_finish(1, "FAILED: connection lost: %s" % reason)


## `error` if there is one, else `ok`.
func _or_ok(error: String, ok: String) -> String:
	return error if error != "" else ok


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
	top_bar.saves_requested.connect(func() -> void: save_menu.open())
	game.add_child(top_bar)
	var body := HBoxContainer.new()
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var map_column := VBoxContainer.new()
	map_column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	map_modes = MapModes.new()
	var start_mode := _arg("--map-mode=")
	map_modes.mode_changed.connect(func(_mode: int, _good: int) -> void: _subscribe())
	map_column.add_child(map_modes)
	map_view = MapView.new()
	map_view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	map_view.province_clicked.connect(_select_province)
	# The bridge decodes the province-ID texels (pax_map's encoding), not GDScript.
	map_view.province_lookup = func(x: int, y: int) -> Variant: return client.province_at(x, y)
	map_column.add_child(map_view)
	province_info = Label.new()
	map_column.add_child(province_info)
	body.add_child(map_column)
	tabs = TabContainer.new()
	tabs.custom_minimum_size.x = 540
	summary = SummaryPanel.new()
	summary.name = "World"
	tabs.add_child(summary)
	nation_panel = NationPanel.new()
	nation_panel.name = "Nation"
	nation_panel.policy_requested.connect(func(policy: String, nation: int, rate_raw: int) -> void:
		client.submit_policy(policy, nation, rate_raw))
	tabs.add_child(nation_panel)
	market_panel = MarketPanel.new()
	market_panel.name = "Market"
	tabs.add_child(market_panel)
	province_panel = ProvincePanel.new()
	province_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var province_scroll := ScrollContainer.new()
	province_scroll.name = "Province"
	province_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	province_scroll.add_child(province_panel)
	tabs.add_child(province_scroll)
	# --tab names a tab (its node name), so adding or reordering tabs can't make it stale.
	var tab := _arg("--tab=")
	if tab != "":
		var page := tabs.get_node_or_null(tab) as Control
		if page != null:
			tabs.current_tab = tabs.get_tab_idx_from_control(page)
	body.add_child(tabs)
	game.add_child(body)
	add_child(game)
	if start_mode.is_valid_int():
		map_modes.select(int(start_mode))

	overlay = DebugOverlay.new()
	add_child(overlay)
	overlay.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_RIGHT, Control.PRESET_MODE_MINSIZE, 8)
	overlay.grow_horizontal = Control.GROW_DIRECTION_BEGIN
	overlay.grow_vertical = Control.GROW_DIRECTION_BEGIN
	# Hidden until F3, except in screenshots, which show what a bug report needs.
	overlay.visible = _arg("--screenshot=") != ""

	save_menu = SaveMenu.new()
	save_menu.refresh_requested.connect(func() -> void: client.list_saves())
	save_menu.save_requested.connect(func(name: String) -> void:
		save_menu.show_status(_or_ok(client.save_game(name), "Saving…")))
	save_menu.load_requested.connect(func(name: String) -> void:
		save_menu.show_status(_or_ok(client.load_game(name), "Loading…")))
	add_child(save_menu)
	save_menu.set_anchors_and_offsets_preset(Control.PRESET_CENTER, Control.PRESET_MODE_MINSIZE)

	lost = LostScreen.new()
	lost.restart_requested.connect(func() -> void:
		if client != null:
			client.disconnect_from_server()
		_start())
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
