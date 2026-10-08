## The Iron and Blood client (M3-8, M4-9, D12): plays alone (it launches a local
## server), hosts a multiplayer game, or joins one; then shows what the server sends.
## It never simulates.
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
##   --smoke                headless check (smoke.gd): a province panel, the Price map, a policy,
##                          and a save, list and load; prints SMOKE OK and quits
##   --host=N               host a game for N players at start (M4-9); with --screenshot, the
##                          screenshot is of the lobby
##   --smoke-host           headless check (smoke_host.gd): host a game over TLS, go through the
##                          lobby, start and play; prints SMOKE OK (host) and quits
class_name ClientApp
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
const Smoke := preload("res://smoke.gd")
const SmokeHost := preload("res://smoke_host.gd")
const SmokeScript := preload("res://smoke_script.gd")
const StartScreen := preload("res://ui/start_screen.gd")
const LobbyScreen := preload("res://ui/lobby_screen.gd")

const SCREENSHOT_DAYS := 30

var client: PaxClient
var welcome: Dictionary = {}
var last_update: Dictionary = {}

var start_screen: StartScreen
var lobby: LobbyScreen
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
## The --smoke or --smoke-host run, or `null`.
var _smoke: SmokeScript = null
## A multiplayer session (hosted or joined). How to reach it again, and the resume
## token, live in the bridge (`PaxClient.rejoin`, D24).
var _multiplayer := false
## The latest LOBBY_STATE: who is host, and the names `waiting_for` refers to.
var _last_lobby: Dictionary = {}
## The province panel's province, or `null` for none (D22: no sentinels).
var selected_province: Variant = null
var _finishing := false


func _ready() -> void:
	_build_ui()
	var error := _open_tab_arg()
	if error != "":
		push_error(error)
		if _flag("--smoke") or _arg("--screenshot=") != "":
			finish(1, "FAILED: " + error)
			return
	if _flag("--smoke-host"):
		_host(2, "Smoke")
	elif _arg("--host=").is_valid_int():
		_host(int(_arg("--host=")), "Host")
	elif _flag("--autostart") or _flag("--smoke") or _arg("--screenshot=") != "":
		_start()


func _process(_delta: float) -> void:
	if client == null or _finishing:
		return
	for event: Dictionary in client.poll():
		if not _finishing:
			_handle(event)
		if _smoke != null and not _finishing:
			_smoke.on_event(event)
	if _smoke != null and not _finishing:
		_smoke.on_frame()


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and event.keycode == KEY_F3:
		overlay.visible = not overlay.visible


## Starts a session's UI. `rejoining`: the same client reconnects (its hosted game,
## if any, lives in it, so it must not be replaced); otherwise a fresh client, and
## freeing the old one ends whatever it ran.
func _begin(multiplayer: bool, rejoining := false) -> void:
	start_screen.visible = false
	game.visible = true
	lost.visible = false
	lobby.visible = false
	_multiplayer = multiplayer
	_session_started = false
	_last_lobby = {}
	welcome = {}
	_reset_session()
	top_bar.set_host(true)
	if client != null:
		client.disconnect_from_server()
	if not rejoining or client == null:
		client = PaxClient.new()


## Single player: the client launches its own server (NETWORK_PROTOCOL §6).
func _start() -> void:
	_begin(false)
	_smoke = Smoke.new(self) if _flag("--smoke") else null
	var error := client.launch(_server_path(), _scenario_dir(), OS.get_user_data_dir().path_join("saves"))
	if error != "":
		_connection_lost(error)
		return
	var nation := _arg("--nation=")
	error = client.hello(int(nation) if nation.is_valid_int() else null)
	if error != "":
		_connection_lost(error)


## Hosts a game for `players` (M4-9): a server on this machine over TLS, which this
## client joins like everyone else; the lobby shows what to share.
func _host(players: int, name: String) -> void:
	_begin(true)
	_smoke = SmokeHost.new(self) if _flag("--smoke-host") else null
	var saves := OS.get_user_data_dir().path_join("saves")
	var error := client.host_game(_server_path(), _scenario_dir(), saves, players, name)
	if error != "":
		return _back_to_start(error)
	lobby.set_hosting(client.hosted())
	_hello(null)


## Joins a game elsewhere (M4-9) over TLS, pinned to the fingerprint the host shared.
func _join_game(host: String, port: int, fingerprint: String, password: String, name: String) -> void:
	_begin(true)
	# The map comes from this client's own copy of the scenario (D12).
	var error := client.join_game(host, port, fingerprint, password, name, _scenario_dir())
	if error != "":
		return _back_to_start(error)
	_hello(null)


func _hello(nation: Variant) -> void:
	var error := client.hello(nation)
	if error != "":
		_connection_lost(error)


## Rejoins a multiplayer game after a drop: the bridge reconnects the way it first
## joined and sends the resume token, which reclaims the seat and its nation (D24).
func _rejoin() -> void:
	_begin(true, true)
	var error := client.rejoin()
	if error != "":
		_connection_lost(error)


func _back_to_start(error: String) -> void:
	game.visible = false
	start_screen.visible = true
	start_screen.show_status(error)
	if _smoke != null:
		finish(1, "SMOKE FAILED: " + error)


## What the host shares (`PaxClient.hosted()`); empty unless this client hosts.
func hosted() -> Dictionary:
	return client.hosted() if client != null else {}


func _handle(event: Dictionary) -> void:
	match event[PaxKeys.TYPE]:
		PaxKeys.WELCOME:
			var reload := _session_started
			_session_started = true
			welcome = event
			if reload:
				# A load replaced the game (D23): its tables may differ, so start afresh.
				_reset_session()
			if _multiplayer:
				lobby.set_session(welcome)
			top_bar.set_session(welcome)
			summary.set_session(welcome)
			nation_panel.set_session(welcome)
			market_panel.set_session(welcome)
			province_panel.set_session(welcome)
			_load_map()
			_subscribe()
			var select := _arg("--select=")
			if select.is_valid_int():
				select_province(int(select))
			if _flag("--open-saves") and not reload:
				save_menu.open()
			if _arg("--screenshot=") != "":
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
			_check_screenshot(event[PaxKeys.DAY])
		PaxKeys.COMMAND_RESULT:
			nation_panel.show_result(event)
		PaxKeys.SERVER_STATE:
			top_bar.show_speed(event[PaxKeys.SPEED])
			top_bar.show_waiting(LobbyScreen.names_of(_last_lobby, event[PaxKeys.WAITING_FOR]))
		PaxKeys.LOBBY_STATE:
			if event[PaxKeys.NOTICE] == null:
				_last_lobby = event
			lobby.show_lobby(event)
			top_bar.set_host(LobbyScreen.is_host(_last_lobby, welcome.get(PaxKeys.PLAYER, -1)))
			# In multiplayer the lobby's claim, not the Welcome, says who this player plays.
			var me := LobbyScreen.me(_last_lobby, welcome.get(PaxKeys.PLAYER, -1))
			if not me.is_empty():
				top_bar.set_playing(me[PaxKeys.NATION], me[PaxKeys.SANDBOX])
				nation_panel.set_playing(null if me[PaxKeys.SANDBOX] else me[PaxKeys.NATION])
			# --host with --screenshot: what a host sees in the lobby.
			var shot := _arg("--screenshot=")
			if shot != "" and not event[PaxKeys.STARTED] and not _finishing:
				_finishing = true
				await get_tree().create_timer(0.3).timeout
				await _save_screenshot(shot)
				finish(0, "")
		PaxKeys.SAVE_LIST:
			save_menu.show_list(event[PaxKeys.NAMES])
		PaxKeys.SAVE_RESULT:
			save_menu.show_result(event)
		PaxKeys.CLOSED:
			_connection_lost(event[PaxKeys.REASON])


## Forgets the old game's per-session UI state, for a new game (`_start`) and a loaded
## one (a later Welcome) alike. The panels reset themselves in `set_session`.
func _reset_session() -> void:
	last_update = {}
	selected_province = null
	map_view.set_selected(null)
	save_menu.visible = false


## The --screenshot run: once the game has run long enough, pause, capture and quit.
func _check_screenshot(day: int) -> void:
	var shot := _arg("--screenshot=")
	if shot == "" or day < SCREENSHOT_DAYS or _finishing:
		return
	_finishing = true
	client.set_speed(PaxKeys.SPEED_PAUSED)
	await get_tree().create_timer(0.3).timeout
	await _save_screenshot(shot)
	finish(0, "")


## Saves what the window shows to `path` and prints the SCREENSHOT line CI reads.
func _save_screenshot(path: String) -> void:
	await RenderingServer.frame_post_draw
	get_viewport().get_texture().get_image().save_png(path)
	print("SCREENSHOT %s" % path)


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
				text += " · price %s" % Format.price(v)
			_:
				text += " · %s" % Format.percent(v)
	province_info.text = text


func select_province(province: int) -> void:
	selected_province = province
	map_view.set_selected(province)
	_subscribe()
	# From the world summary, a selection opens the province's own panel.
	if tabs.get_current_tab_control() == summary:
		tabs.current_tab = tabs.get_tab_idx_from_control(tabs.get_node("Province"))


func _connection_lost(reason: String) -> void:
	# A multiplayer seat waits for its resume token (D24): the player can rejoin.
	var hosting := client != null and not client.hosted().is_empty()
	lost.show_reason(reason, client != null and client.can_rejoin(), hosting)
	save_menu.visible = false
	lobby.visible = false
	var shot := _arg("--screenshot=")
	if shot != "" and not _finishing:
		# Show what the player would see, then fail the run.
		_finishing = true
		await _save_screenshot(shot)
	if _flag("--smoke") or _flag("--smoke-host") or shot != "":
		finish(1, "FAILED: connection lost: %s" % reason)


## `error` if there is one, else `ok`.
func _or_ok(error: String, ok: String) -> String:
	return error if error != "" else ok


## Ends a scripted run (--smoke, --screenshot) with `code`, printing `message`.
func finish(code: int, message: String) -> void:
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

	start_screen = StartScreen.new()
	start_screen.set_scenario(_scenario_dir().get_file())
	start_screen.single_player_requested.connect(_start)
	start_screen.host_requested.connect(_host)
	start_screen.join_requested.connect(_join_game)
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
	map_view.province_clicked.connect(select_province)
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
		client = null
		game.visible = false
		lost.visible = false
		start_screen.visible = true)
	lost.rejoin_requested.connect(_rejoin)

	lobby = LobbyScreen.new()
	lobby.claim_requested.connect(func(nation: Variant) -> void: client.claim_nation(nation))
	lobby.ready_requested.connect(func(ready: bool) -> void: client.set_ready(ready))
	lobby.start_requested.connect(func() -> void: client.start_game())
	lobby.kick_requested.connect(func(player: int) -> void: client.kick(player))
	add_child(lobby)
	# The connection-lost screen goes over the lobby.
	add_child(lost)


## Opens the --tab=NAME tab. NAME is a tab's node name, so adding or reordering tabs
## can't make it stale. Returns an error for a name that matches no tab: like the
## bridge's arguments, it is reported, never quietly ignored.
func _open_tab_arg() -> String:
	var tab := _arg("--tab=")
	if tab == "":
		return ""
	var page := tabs.get_node_or_null(tab) as Control
	if page == null:
		var names := PackedStringArray()
		for child in tabs.get_children():
			names.append(child.name)
		return "--tab=%s is not a tab (%s)" % [tab, ", ".join(names)]
	tabs.current_tab = tabs.get_tab_idx_from_control(page)
	return ""


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
