/* A small C library for testing `hxn import`: an opaque object with a
 * constructor and destructor, text and bytes in and out, an enum, a struct
 * passed by value and by pointer, and functions that report errors through
 * their return value. */
#ifndef TALLY_H
#define TALLY_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

typedef struct tally tally;

typedef enum tally_mode {
	TALLY_MODE_COUNT = 0,
	TALLY_MODE_SUM = 1,
	TALLY_MODE_MAX = 5
} tally_mode;

typedef struct tally_point {
	double x;
	double y;
} tally_point;

/* Returns NULL when `name` is empty. */
tally *tally_new(const char *name, tally_mode mode);
void tally_free(tally *t);

/* Returns 0 on success, -1 when the total would overflow. */
int tally_add(tally *t, int32_t amount);
int64_t tally_total(const tally *t);
const char *tally_name(const tally *t);
tally_mode tally_mode_of(const tally *t);
void tally_set_mode(tally *t, tally_mode mode);
/* Adds every byte of `data`. */
size_t tally_add_bytes(tally *t, const uint8_t *data, size_t length);
/* A new string the caller frees with tally_string_free. */
char *tally_describe(const tally *t);
void tally_string_free(char *text);
bool tally_is_empty(const tally *t);

/* The Manhattan distance between two points. */
double tally_distance(tally_point a, const tally_point *b);
tally_point tally_midpoint(tally_point a, tally_point b);
unsigned long tally_version(void);
/* Writes the integer `text` holds to `out`. Returns 0 on success, -1 when
 * `text` is not an integer. */
int tally_parse(const char *text, int *out);
/* Writes a new tally with `t`'s total and mode to `out`. Returns 0 on
 * success, -1 when `name` is empty. */
int tally_copy(const tally *t, const char *name, tally **out);
/* Writes the point (0, 0) to `out`. */
void tally_origin(tally_point *out);

/* Called with each amount added, oldest first. Returning non-zero stops. */
typedef int (*tally_visitor)(void *user, int32_t amount);
/* Calls `visit` with each amount added, up to the last 16, oldest first, and
 * returns how many it was called with. */
int tally_each(const tally *t, tally_visitor visit, void *user);
/* Called with each line of a report. */
typedef void (*tally_reporter)(const char *line, void *user);
/* Reports the total and the count, a line each. */
void tally_report(const tally *t, tally_reporter report, void *user);

#endif
