## The world at a glance: the day's `WorldSummary` and the `NationTable` (D22).
## Display only: every number is formatted here, never sent back.
extends PanelContainer

const PaxKeys := preload("res://pax_keys.gd")
const Format := preload("res://ui/format.gd")

var _world: Label
var _nations: GridContainer
var _nation_names := PackedStringArray()


func _init() -> void:
	var column := VBoxContainer.new()
	_world = Label.new()
	column.add_child(_world)
	column.add_child(HSeparator.new())
	_nations = GridContainer.new()
	_nations.columns = 6
	_nations.add_theme_constant_override("h_separation", 24)
	column.add_child(_nations)
	add_child(column)


func set_session(welcome: Dictionary) -> void:
	_nation_names = welcome[PaxKeys.NATIONS]


func show_update(update: Dictionary) -> void:
	var w: Dictionary = update[PaxKeys.WORLD]
	var workforce: int = w[PaxKeys.WORKFORCE]
	var unemployment := 0.0 if workforce == 0 else float(w[PaxKeys.UNEMPLOYED]) / workforce
	_world.text = "\n".join([
		"Population %s   ·   unemployment %s   ·   deprived %s" % [
			Format.count(w[PaxKeys.POPULATION]), Format.percent(unemployment), Format.count(w[PaxKeys.DEPRIVED])],
		"Life needs %s   ·   militancy %s" % [Format.percent(w[PaxKeys.LIFE_NEEDS]), Format.percent(w[PaxKeys.MILITANCY])],
		"Today: households spent %s, governments %s, producers %s on inputs" % [
			Format.money(w[PaxKeys.HOUSEHOLD_SPENDING]), Format.money(w[PaxKeys.GOVERNMENT_SPENDING]), Format.money(w[PaxKeys.INPUT_SPENDING])],
		"Wages %s, dividends %s, taxes %s, transfers %s" % [
			Format.money(w[PaxKeys.WAGES]), Format.money(w[PaxKeys.DIVIDENDS]), Format.money(w[PaxKeys.TAXES]), Format.money(w[PaxKeys.TRANSFERS])],
	])

	var n: Dictionary = update[PaxKeys.NATIONS]
	for child in _nations.get_children():
		child.queue_free()
	for heading in ["Nation", "Population", "Treasury", "Income tax", "Transfers", "Consumption"]:
		_cell(heading)
	for i in _nation_names.size():
		_cell(_nation_names[i].capitalize())
		_cell(Format.count(n[PaxKeys.POPULATION][i]))
		_cell(Format.money(n[PaxKeys.TREASURY][i]))
		_cell(Format.rate(n[PaxKeys.INCOME_TAX_RATE_RAW][i]))
		_cell(Format.rate(n[PaxKeys.TRANSFER_RATE_RAW][i]))
		_cell(Format.rate(n[PaxKeys.CONSUMPTION_RATE_RAW][i]))


func _cell(text: String) -> void:
	var l := Label.new()
	l.text = text
	_nations.add_child(l)
