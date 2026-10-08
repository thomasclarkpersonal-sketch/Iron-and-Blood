## Number formatting for display. Presentation only: nothing here is sent back.


## A 64-bit hash (an identifier, carried bit-for-bit in Godot's signed int) as 16
## hex digits, as the server and `pax_cli replay` print it.
static func hash_hex(h: int) -> String:
	return "%08x%08x" % [(h >> 32) & 0xffffffff, h & 0xffffffff]


static func count(n: int) -> String:
	if n >= 1_000_000:
		return "%.2fM" % (n / 1_000_000.0)
	if n >= 10_000:
		return "%.1fk" % (n / 1_000.0)
	return str(n)


static func money(x: float) -> String:
	return "£" + count(int(round(x)))


static func percent(x: float) -> String:
	return "%.1f%%" % (x * 100.0)


## A raw Fixed rate (value × 10⁶) as a percentage.
static func rate(raw: int) -> String:
	return "%.1f%%" % (raw / 10_000.0)
