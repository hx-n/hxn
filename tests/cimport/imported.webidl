// The API of a C library, for hxn. hxn generates the library's Rust bindings
// from this file: edit it to record what the header cannot, such as which
// function frees a returned pointer, or [HxnStatus] on a function whose
// return value is a status code.

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
  [HxnC=tally_add] long add(long amount);
  [HxnC=tally_total] long long total();
  [HxnC=tally_name] DOMString? name();
  [HxnC=tally_mode_of] Mode mode_of();
  [HxnC=tally_set_mode] undefined set_mode(Mode mode);
  // Adds every byte of `data`.
  [HxnC=tally_add_bytes, HxnReturns=size_t] unsigned long long add_bytes(Uint8Array data);
  // A new string the caller frees with tally_string_free.
  [HxnC=tally_describe, HxnFree=tally_string_free] DOMString? describe();
  [HxnC=tally_is_empty] boolean is_empty();
};

namespace Tally {
  // The Manhattan distance between two points.
  [HxnC=tally_distance] double distance(Point a, [HxnRef] Point b);
  [HxnC=tally_midpoint] Point midpoint(Point a, Point b);
  [HxnC=tally_version, HxnReturns=unsigned_long] unsigned long long version();
};

// Not imported:
// - tally_parse: `out`: `int *` may be an out-parameter or an array, which is not imported
