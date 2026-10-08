## The nation panel (M3-8c): a nation's figures and its three policy sliders
## (income tax, transfers, consumption; D15, D16). The player's nation, or any nation
## in sandbox.
##
## A slider is in tenths of a percent (per mille), so its value is an integer, and
## the bridge converts it to and from the command's raw `Fixed` rate (`rate_per_mille`,
## `rate_from_per_mille`): never through a float, and GDScript never does Fixed
## arithmetic (D3). The command
## goes when the player releases the slider. The panel shows what the server said
## (`CommandResult`), and the slider follows the server's rate whenever it isn't being
## dragged.
extends VBoxContainer

const PaxKeys := preload("res://pax_keys.gd")
const Format := preload("res://ui/format.gd")

## The player set `policy` (a `PaxKeys.POLICY_*`) of `nation` to `rate_raw`.
signal policy_requested(policy: String, nation: int, rate_raw: int)

## Each slider: policy, label, column in the NationTable.
const POLICIES := [
	[PaxKeys.POLICY_INCOME_TAX, "Income tax", PaxKeys.INCOME_TAX_RATE_RAW],
	[PaxKeys.POLICY_TRANSFER, "Transfers (share of treasury per day)", PaxKeys.TRANSFER_RATE_RAW],
	[PaxKeys.POLICY_CONSUMPTION, "Government spending (share of treasury per day)", PaxKeys.CONSUMPTION_RATE_RAW],
]

var _nation := 0
var _chooser: OptionButton
var _figures: Label
var _sliders: Array[HSlider] = []
## Whether each slider is being dragged: then it doesn't follow the server's rate.
var _dragging: Array[bool] = []
var _values: Array[Label] = []
var _status: Label
var _last_update: Dictionary = {}


func _init() -> void:
	_chooser = OptionButton.new()
	_chooser.item_selected.connect(func(i: int) -> void:
		_nation = i
		if not _last_update.is_empty():
			show_update(_last_update))
	add_child(_chooser)
	_figures = Label.new()
	add_child(_figures)
	add_child(HSeparator.new())
	for entry in POLICIES:
		var label := Label.new()
		label.text = entry[1]
		add_child(label)
		var row := HBoxContainer.new()
		var slider := HSlider.new()
		slider.min_value = 0
		slider.max_value = 1000
		slider.step = 1
		slider.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		var value := Label.new()
		value.custom_minimum_size.x = 64
		var policy: String = entry[0]
		slider.value_changed.connect(func(v: float) -> void:
			value.text = Format.rate(PaxClient.rate_from_per_mille(int(v))))
		var index := _sliders.size()
		slider.drag_started.connect(func() -> void: _dragging[index] = true)
		slider.drag_ended.connect(func(changed: bool) -> void:
			_dragging[index] = false
			if changed:
				policy_requested.emit(policy, _nation, PaxClient.rate_from_per_mille(int(slider.value))))
		row.add_child(slider)
		row.add_child(value)
		add_child(row)
		_sliders.append(slider)
		_dragging.append(false)
		_values.append(value)
	_status = Label.new()
	_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	add_child(_status)


## The session's nations; `playing` is the player's nation, or `null` for sandbox,
## where any nation can be chosen.
func set_session(welcome: Dictionary) -> void:
	_chooser.clear()
	for key in welcome[PaxKeys.NATIONS]:
		_chooser.add_item((key as String).capitalize())
	var playing = welcome[PaxKeys.NATION]
	_nation = 0 if playing == null else int(playing)
	_chooser.select(_nation)
	_chooser.disabled = playing != null


func show_update(update: Dictionary) -> void:
	_last_update = update
	var t: Dictionary = update[PaxKeys.NATION_TABLE]
	_figures.text = "Population %s · treasury %s" % [
		Format.count(t[PaxKeys.POPULATION][_nation]), Format.money(t[PaxKeys.TREASURY][_nation])]
	for i in POLICIES.size():
		var slider := _sliders[i]
		if not _dragging[i]:
			# The server's rate, at the slider step at or below it.
			slider.set_value_no_signal(PaxClient.rate_per_mille(t[POLICIES[i][2]][_nation]))
			_values[i].text = Format.rate(t[POLICIES[i][2]][_nation])


## What the server said about a command this panel sent.
func show_result(result: Dictionary) -> void:
	var error: int = result[PaxKeys.COMMAND_ERROR]
	if error == PaxKeys.COMMAND_ERROR_NONE:
		_status.text = "Accepted: applies at the start of day %d." % result[PaxKeys.APPLIES_ON_DAY]
	else:
		_status.text = "Refused: %s." % PaxKeys.COMMAND_ERROR_NAMES[error]
