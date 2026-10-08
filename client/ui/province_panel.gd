## The province panel (M3-8c): the selected province's POPs by profession (D7: a
## POP is its `(province, profession)` identity), its labour pools and its producers
## (`ProvinceDetail`, D22).
extends VBoxContainer

const PaxKeys := preload("res://pax_keys.gd")
const Format := preload("res://ui/format.gd")
const Table := preload("res://ui/table.gd")

var _title: Label
var _pops: Table
var _labour: Table
var _producers: Table
var _welcome: Dictionary


func _init() -> void:
	_title = Label.new()
	_title.text = "Select a province on the map."
	add_child(_title)
	for heading in ["People", "Labour", "Producers"]:
		var l := Label.new()
		l.text = heading
		l.add_theme_font_size_override("font_size", 18)
		add_child(l)
		var t := Table.new()
		add_child(t)
		match heading:
			"People":
				_pops = t
			"Labour":
				_labour = t
			_:
				_producers = t


func set_session(welcome: Dictionary) -> void:
	_welcome = welcome


func show_update(update: Dictionary) -> void:
	var p = update[PaxKeys.PROVINCE]
	if p == null:
		return
	var professions: PackedStringArray = _welcome[PaxKeys.PROFESSIONS]
	var types: PackedStringArray = _welcome[PaxKeys.PRODUCER_TYPES]
	_title.text = "Province: %s" % (_welcome[PaxKeys.PROVINCES][p[PaxKeys.PROVINCE_ID]] as String).capitalize()

	var pops: Dictionary = p[PaxKeys.POPS]
	var rows := []
	for i in pops[PaxKeys.PROFESSION].size():
		rows.append([professions[pops[PaxKeys.PROFESSION][i]].capitalize(), Format.count(pops[PaxKeys.PEOPLE][i]),
			Format.money(pops[PaxKeys.CASH][i]), Format.percent(pops[PaxKeys.LIFE_NEEDS][i]),
			Format.percent(pops[PaxKeys.MILITANCY][i])])
	_pops.fill(["Profession", "People", "Cash", "Life needs", "Militancy"], rows)

	var labour: Dictionary = p[PaxKeys.LABOUR]
	rows = []
	for i in labour[PaxKeys.PROFESSION].size():
		rows.append([professions[labour[PaxKeys.PROFESSION][i]].capitalize(), Format.count(labour[PaxKeys.WORKFORCE][i]),
			Format.count(labour[PaxKeys.JOBS][i]), Format.count(labour[PaxKeys.EMPLOYED][i])])
	_labour.fill(["Profession", "Workforce", "Jobs", "Employed"], rows)

	var producers: Dictionary = p[PaxKeys.PRODUCERS]
	rows = []
	for i in producers[PaxKeys.PRODUCER_TYPE].size():
		rows.append([types[producers[PaxKeys.PRODUCER_TYPE][i]].capitalize(),
			"%d / %d" % [producers[PaxKeys.EMPLOYED][i], producers[PaxKeys.CAPACITY][i]],
			"%.2f" % producers[PaxKeys.WAGE][i], Format.money(producers[PaxKeys.CASH][i])])
	_producers.fill(["Producer", "Employed", "Wage", "Cash"], rows)
