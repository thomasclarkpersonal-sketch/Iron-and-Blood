## Colours for each map mode (D22, NETWORK_PROTOCOL §4). Presentation only.

const PaxKeys := preload("res://pax_keys.gd")
const Format := preload("res://ui/format.gd")

const STATELESS := Color(0.55, 0.55, 0.55)


## Nation mode, drawn from `Welcome` alone (the server sends no values): each
## province takes the colour of the nation owning its market.
static func nations(welcome: Dictionary) -> PackedColorArray:
	var owner := {}
	var nation_markets: Array = welcome[PaxKeys.NATION_MARKETS]
	for n in nation_markets.size():
		for m in nation_markets[n]:
			owner[m] = n
	var out := PackedColorArray()
	for m in welcome[PaxKeys.PROVINCE_MARKET]:
		out.append(nation_color(owner[m]) if owner.has(m) else STATELESS)
	return out


## A distinct, stable colour per nation index (golden-ratio hues).
static func nation_color(nation: int) -> Color:
	return Color.from_hsv(fmod(0.08 + nation * 0.618034, 1.0), 0.55, 0.85)


## A value mode: one colour per province on the mode's scale.
static func values(mode: int, values: PackedFloat64Array) -> PackedColorArray:
	var lo := 0.0
	var hi := 1.0
	if mode == PaxKeys.MAP_MODE_POPULATION or mode == PaxKeys.MAP_MODE_PRICE:
		lo = INF
		hi = -INF
		for v in values:
			lo = minf(lo, v)
			hi = maxf(hi, v)
	var out := PackedColorArray()
	for v in values:
		var t := 0.0 if hi <= lo else clampf((v - lo) / (hi - lo), 0.0, 1.0)
		out.append(ramp(mode, t))
	return out


## Low to high along a scale that reads as bad or good for the mode.
static func ramp(mode: int, t: float) -> Color:
	match mode:
		PaxKeys.MAP_MODE_LIFE_NEEDS: # high is good
			return Color.from_hsv(0.33 * t, 0.7, 0.85)
		PaxKeys.MAP_MODE_UNEMPLOYMENT, PaxKeys.MAP_MODE_MILITANCY: # high is bad
			return Color.from_hsv(0.33 * (1.0 - t), 0.7, 0.85)
		_: # neutral magnitude
			return Color.from_hsv(0.6, 0.15 + 0.7 * t, 0.95 - 0.45 * t)


## The legend for a value mode: what the colours span.
static func legend(mode: int, values: PackedFloat64Array, good: String) -> String:
	if mode == PaxKeys.MAP_MODE_NATION:
		return "Nations"
	var lo := INF
	var hi := -INF
	for v in values:
		lo = minf(lo, v)
		hi = maxf(hi, v)
	if values.is_empty():
		return ""
	match mode:
		PaxKeys.MAP_MODE_POPULATION:
			return "Population %s – %s" % [Format.count(int(lo)), Format.count(int(hi))]
		PaxKeys.MAP_MODE_PRICE:
			return "Price of %s %s – %s" % [good.capitalize(), Format.price(lo), Format.price(hi)]
		PaxKeys.MAP_MODE_UNEMPLOYMENT:
			return "Unemployment 0% (green) – 100% (red)"
		PaxKeys.MAP_MODE_LIFE_NEEDS:
			return "Life needs 0% (red) – 100% (green)"
		PaxKeys.MAP_MODE_MILITANCY:
			return "Militancy 0% (green) – 100% (red)"
	return ""
