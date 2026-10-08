## The market panel (M3-8c): every good's price, supply, demand and traded volume
## in the selected province's market (`MarketDetail`, D22).
extends VBoxContainer

const PaxKeys := preload("res://pax_keys.gd")
const Format := preload("res://ui/format.gd")
const Table := preload("res://ui/table.gd")

var _title: Label
var _table: Table
var _goods := PackedStringArray()
var _markets := PackedStringArray()


func _init() -> void:
	_title = Label.new()
	_title.text = "Select a province to see its market."
	add_child(_title)
	_table = Table.new()
	add_child(_table)


func set_session(welcome: Dictionary) -> void:
	_goods = welcome[PaxKeys.GOODS]
	_markets = welcome[PaxKeys.MARKETS]


func show_update(update: Dictionary) -> void:
	var m = update[PaxKeys.MARKET]
	if m == null:
		return
	_title.text = "Market: %s" % _markets[m[PaxKeys.MARKET_ID]].capitalize()
	var rows := []
	for g in _goods.size():
		rows.append([_goods[g].capitalize(), Format.price(m[PaxKeys.PRICE][g]), Format.amount(m[PaxKeys.SUPPLY][g]),
			Format.amount(m[PaxKeys.DEMAND][g]), Format.amount(m[PaxKeys.TRADED][g])])
	_table.fill(["Good", "Price", "Supply", "Demand", "Traded"], rows)
