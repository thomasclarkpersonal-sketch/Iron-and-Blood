## What main.gd calls on a smoke run (`--smoke`, `--smoke-host`). Both smoke scripts
## extend this, and main.gd holds the run as this type, so a misspelt call fails when
## the scripts load, not mid-run in CI.
extends RefCounted


## Every event, after the UI has handled it.
func on_event(_event: Dictionary) -> void:
	pass


## Once a frame, after the events.
func on_frame() -> void:
	pass
