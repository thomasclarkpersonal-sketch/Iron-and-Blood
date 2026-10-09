## A simple text table: a GridContainer refilled with headings and rows of strings.
extends GridContainer


func _init() -> void:
	add_theme_constant_override("h_separation", 14)


func fill(headings: Array, rows: Array) -> void:
	for child in get_children():
		remove_child(child)
		child.queue_free()
	columns = headings.size()
	for h in headings:
		var l := Label.new()
		l.text = h
		l.add_theme_color_override("font_color", Color(0.75, 0.8, 0.9))
		add_child(l)
	for row in rows:
		for cell in row:
			var l := Label.new()
			l.text = cell
			add_child(l)
