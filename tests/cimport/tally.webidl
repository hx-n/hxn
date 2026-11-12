// The API of a C library, for hxn. hxn generates the library's Rust bindings
// from this file: edit it to record what the header cannot, such as which
// function frees a returned pointer, or [HxnStatus] on a function whose
// return value is a status code.

// Called with each amount added, oldest first. Returning non-zero stops.
[HxnUser=first, HxnStop=1]
callback Visitor = long (long value);

// Called with each line of a report.
[HxnUser=last]
callback Reporter = undefined (DOMString value);

// Called after each amount is added, with the tally's name, the amount and
// the new total.
[HxnUser=first]
callback Listener = undefined (DOMString name, long amount, long long total);

[HxnEnum]
interface Mode {
  const long Count = 0;
  const long Sum = 1;
  const long Max = 5;
};

dictionary Point {
  required double x;
  required double y;
};

[HxnFree=tally_free]
interface Tally {
  // Returns NULL when `name` is empty.
  [HxnC=tally_new] constructor(DOMString name, Mode mode);
  // Returns 0 on success, -1 when the total would overflow.
  [HxnC=tally_add, HxnStatus] undefined add(long amount);
  [HxnC=tally_total] long long total();
  [HxnC=tally_name] DOMString? name();
  [HxnC=tally_mode_of] Mode mode_of();
  [HxnC=tally_set_mode] undefined set_mode(Mode mode);
  // Adds every byte of `data`.
  [HxnC=tally_add_bytes, HxnReturns=size_t] unsigned long long add_bytes(Uint8Array data);
  // A new string the caller frees with tally_string_free.
  [HxnC=tally_describe, HxnFree=tally_string_free] DOMString? describe();
  [HxnC=tally_is_empty] boolean is_empty();
  // Writes a new tally with `t`'s total and mode to `out`. Returns 0 on
  // success, -1 when `name` is empty.
  [HxnC=tally_copy, HxnStatus] Tally copy(DOMString name, [HxnOut] Tally out);
  // Calls `visit` with each amount added, up to the last 16, oldest first, and
  // returns how many it was called with.
  [HxnC=tally_each] long each(Visitor visit);
  // Reports the total and the count, a line each.
  [HxnC=tally_report] undefined report(Reporter report);
  // Calls `listener` after each amount is added until the tally is freed, in
  // place of the listener before it. Returns the user data of the listener it
  // replaces, or NULL.
  [HxnC=tally_listen, HxnIgnore=pointer] undefined listen([HxnKept] Listener listener);
};

namespace Tally {
  // The Manhattan distance between two points.
  [HxnC=tally_distance] double distance(Point a, [HxnRef] Point b);
  [HxnC=tally_midpoint] Point midpoint(Point a, Point b);
  [HxnC=tally_version, HxnReturns=unsigned_long] unsigned long long version();
  // Writes the integer `text` holds to `out`. Returns 0 on success, -1 when
  // `text` is not an integer.
  [HxnC=tally_parse, HxnStatus] long parse(DOMString text, [HxnOut] long out);
  // Writes the point (0, 0) to `out`.
  [HxnC=tally_origin] Point origin([HxnOut] Point out);
};

// Check:
// - tally_parse: `out` is taken as an output ([HxnOut]), and its int return value as a status ([HxnStatus])
// - tally_copy: `out` is taken as an output ([HxnOut]), and its int return value as a status ([HxnStatus])
// - tally_origin: `out` is taken as an output ([HxnOut])
// - tally_listen: the pointer it returns is ignored ([HxnIgnore=pointer]): the binding does not give back what `listener` replaced
// - tally_each: C must call `visit` only before tally_each returns; mark it [HxnKept] if C keeps it
// - tally_report: C must call `report` only before tally_report returns; mark it [HxnKept] if C keeps it
// - tally_listen: C is taken to keep `listener` ([HxnKept]), and the binding returns a stream of what C calls it with
// - Visitor: when the Haxe function throws, C is returned 1 ([HxnStop=1])
