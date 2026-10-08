## The debug overlay (F3): what a bug report needs (D23). The `state_hash` pins the
## exact server state, so the save's command log can replay to it.
extends Label

const PaxKeys := preload("res://pax_keys.gd")
const Format := preload("res://ui/format.gd")

var _updates := 0
var _window_start_ms := 0
var _rate := 0.0


func _init() -> void:
	add_theme_color_override("font_color", Color(0.7, 1.0, 0.7))
	add_theme_color_override("font_outline_color", Color.BLACK)
	add_theme_constant_override("outline_size", 4)
	text = "F3: debug overlay"


func show_update(update: Dictionary) -> void:
	_updates += 1
	var now := Time.get_ticks_msec()
	if now - _window_start_ms >= 1000:
		_rate = _updates * 1000.0 / maxi(now - _window_start_ms, 1)
		_updates = 0
		_window_start_ms = now
	text = "day %d   state %s   skipped %d   %.1f updates/s" % [
		update[PaxKeys.DAY], Format.hash_hex(update[PaxKeys.STATE_HASH]), update[PaxKeys.SKIPPED], _rate]
