## The headless smoke test (`--smoke`, CI): drives a game through the client's real
## paths and checks what comes back. main.gd routes every event to the UI first, then
## hands it here, so the smoke test never changes what the player's client does.
##
## The run: select province 0, switch to the Price map, set a policy, save at day
## SAVE_DAY, list the saves, load the save, run on, then check that the reloaded
## game still has the subscribed views and the policy, and print SMOKE OK.
extends "res://smoke_script.gd"

const PaxKeys := preload("res://pax_keys.gd")
const Format := preload("res://ui/format.gd")

## How far the run has got.
enum Stage { STARTING, SAVING, LISTING, LOADING, RELOADED }

const DAYS := 40
const TIMEOUT_MS := 60_000
const SAVE := "smoke"
const SAVE_DAY := 20
## Days the reloaded game must run before the final check.
const DAYS_AFTER_RELOAD := 5
## The policy the run sets (income tax of nation 0), in per mille.
const TAX_PER_MILLE := 123

## main.gd, typed, so a renamed member fails when the script loads, not mid-run.
var _app: ClientApp
var _stage := Stage.STARTING
var _started_ms := Time.get_ticks_msec()
## The day the load returned to: the final check needs days after it.
var _reload_day := 0
## The policy command, once sent: its `client_seq`.
var _command: Variant = null


func _init(app: ClientApp) -> void:
	_app = app


## Every frame: fails a run that has stalled.
func on_frame() -> void:
	if Time.get_ticks_msec() - _started_ms > TIMEOUT_MS:
		_fail("stalled at %s after %d s" % [Stage.keys()[_stage], TIMEOUT_MS / 1000])


func on_event(event: Dictionary) -> void:
	var client: PaxClient = _app.client
	match event[PaxKeys.TYPE]:
		PaxKeys.WELCOME:
			if _stage == Stage.LOADING:
				_stage = Stage.RELOADED
				_reload_day = event[PaxKeys.DAY]
			elif _stage != Stage.STARTING:
				_fail("a Welcome at %s" % Stage.keys()[_stage])
				return
			# Exercise the subscription path: a province panel and a value map mode.
			# A load resets the UI, so the reloaded game needs them again.
			_app.select_province(0)
			_app.map_modes.select(PaxKeys.MAP_MODE_PRICE)
			if _command == null:
				_command = client.submit_policy(PaxKeys.POLICY_INCOME_TAX, 0,
					PaxClient.rate_from_per_mille(TAX_PER_MILLE))
			client.set_speed(PaxKeys.SPEED_FASTEST)
		PaxKeys.DAY_UPDATE:
			_on_day(event)
		PaxKeys.COMMAND_RESULT:
			if event[PaxKeys.CLIENT_SEQ] == _command and event[PaxKeys.COMMAND_ERROR] != PaxKeys.COMMAND_ERROR_NONE:
				_fail("the policy command was refused: %s" % event)
		PaxKeys.SAVE_RESULT:
			# A successful load answers with a Welcome, so any error here fails the run.
			if event[PaxKeys.ERROR] != "":
				_fail("%s %s: %s" % [event[PaxKeys.REQUEST], event[PaxKeys.NAME], event[PaxKeys.ERROR]])
			elif _stage == Stage.SAVING and event[PaxKeys.REQUEST] == PaxKeys.REQUEST_SAVE:
				_stage = Stage.LISTING
				client.list_saves()
		PaxKeys.SAVE_LIST:
			if _stage != Stage.LISTING:
				return
			if not SAVE in event[PaxKeys.NAMES]:
				_fail("the save list lacks %s: %s" % [SAVE, event[PaxKeys.NAMES]])
				return
			_stage = Stage.LOADING
			client.load_game(SAVE)


func _on_day(update: Dictionary) -> void:
	var day: int = update[PaxKeys.DAY]
	if _stage == Stage.STARTING and day >= SAVE_DAY:
		_stage = Stage.SAVING
		_app.client.save_game(SAVE)
	elif _stage == Stage.RELOADED and day >= maxi(DAYS, _reload_day + DAYS_AFTER_RELOAD):
		_check(update)


## The final check, on the reloaded game.
func _check(update: Dictionary) -> void:
	var map = update[PaxKeys.MAP]
	var province = update[PaxKeys.PROVINCE]
	if map == null or map[PaxKeys.MODE] != PaxKeys.MAP_MODE_PRICE or province == null or province[PaxKeys.PROVINCE_ID] != 0:
		_fail("the update lacks the subscribed views: map %s, province %s" % [map, province])
		return
	# The nation panel shows militancy, which this server sends (protocol 1.2).
	if update[PaxKeys.NATION_TABLE][PaxKeys.MILITANCY] == null:
		_fail("the nation table lacks militancy")
		return
	var tax: int = update[PaxKeys.NATION_TABLE][PaxKeys.INCOME_TAX_RATE_RAW][0]
	if tax != PaxClient.rate_from_per_mille(TAX_PER_MILLE):
		_fail("the income tax policy didn't apply (rate %d)" % tax)
		return
	print("SMOKE OK: day %d, state %s, %s, price map, province panel, a policy, save and load" % [
		update[PaxKeys.DAY], Format.hash_hex(update[PaxKeys.STATE_HASH]), _app.welcome[PaxKeys.SCENARIO]])
	_app.finish(0, "")


func _fail(message: String) -> void:
	_app.finish(1, "SMOKE FAILED: " + message)
