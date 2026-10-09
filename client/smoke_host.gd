## The headless multiplayer check (`--smoke-host`, CI, M4-9): host a game over TLS
## the way a player would, go through the lobby, start, and play. main.gd routes
## every event to the UI first, then hands it here.
##
## The run: host for two (the server makes a certificate, and the client pins it),
## claim nation 0 and get ready, start alone (the second seat stays free), set the
## speed, play a few days, and print SMOKE OK.
extends "res://smoke_script.gd"

const PaxKeys := preload("res://pax_keys.gd")
const Format := preload("res://ui/format.gd")
const LobbyScreen := preload("res://ui/lobby_screen.gd")

enum Stage { JOINING, READYING, STARTING, PLAYING }

const DAYS := 10
const TIMEOUT_MS := 60_000

## main.gd, typed, so a renamed member fails when the script loads, not mid-run.
var _app: ClientApp
var _stage := Stage.JOINING
var _started_ms := Time.get_ticks_msec()


func _init(app: ClientApp) -> void:
	_app = app


func on_frame() -> void:
	if Time.get_ticks_msec() - _started_ms > TIMEOUT_MS:
		_fail("stalled at %s after %d s" % [Stage.keys()[_stage], TIMEOUT_MS / 1000])


func on_event(event: Dictionary) -> void:
	var client: PaxClient = _app.client
	match event[PaxKeys.TYPE]:
		PaxKeys.WELCOME:
			if _app.hosted().is_empty():
				return _fail("a hosted game has no port or fingerprint to share")
			_stage = Stage.READYING
			client.claim_nation(0)
		PaxKeys.LOBBY_STATE:
			if event[PaxKeys.NOTICE] != null:
				return _fail("the lobby refused: %s" % event[PaxKeys.NOTICE])
			if event[PaxKeys.STARTED] and _stage == Stage.STARTING:
				_stage = Stage.PLAYING
				client.set_speed(PaxKeys.SPEED_FASTEST)
				return
			var me := LobbyScreen.me(event, _app.welcome[PaxKeys.PLAYER])
			if me.is_empty():
				return
			if _stage == Stage.READYING and me[PaxKeys.NATION] == 0 and not me[PaxKeys.READY]:
				client.set_ready(true)
			elif _stage == Stage.READYING and me[PaxKeys.READY]:
				if not me[PaxKeys.HOST]:
					return _fail("the hosting player isn't the host")
				_stage = Stage.STARTING
				client.start_game()
		PaxKeys.DAY_UPDATE:
			if _stage == Stage.PLAYING and event[PaxKeys.DAY] >= DAYS:
				print("SMOKE OK (host): day %d, state %s, %s, over TLS, through the lobby" % [
					event[PaxKeys.DAY], Format.hash_hex(event[PaxKeys.STATE_HASH]), _app.welcome[PaxKeys.SCENARIO]])
				_app.finish(0, "")


func _fail(message: String) -> void:
	_app.finish(1, "SMOKE FAILED (host): " + message)
