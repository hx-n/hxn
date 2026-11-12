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
/* Not imported: a pointer to an int is an out-parameter. */
int tally_parse(const char *text, int *out);

#endif
