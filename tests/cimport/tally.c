#include "tally.h"

#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct tally {
	char name[64];
	tally_mode mode;
	int64_t total;
	int64_t count;
};

tally *tally_new(const char *name, tally_mode mode) {
	if (!name || !*name)
		return NULL;
	tally *t = calloc(1, sizeof *t);
	if (!t)
		return NULL;
	strncpy(t->name, name, sizeof t->name - 1);
	t->mode = mode;
	return t;
}

void tally_free(tally *t) {
	free(t);
}

int tally_add(tally *t, int32_t amount) {
	switch (t->mode) {
	case TALLY_MODE_COUNT:
		t->total += 1;
		break;
	case TALLY_MODE_SUM:
		if ((amount > 0 && t->total > INT32_MAX - amount) || (amount < 0 && t->total < INT32_MIN - amount))
			return -1;
		t->total += amount;
		break;
	case TALLY_MODE_MAX:
		if (t->count == 0 || amount > t->total)
			t->total = amount;
		break;
	}
	t->count += 1;
	return 0;
}

int64_t tally_total(const tally *t) {
	return t->total;
}

const char *tally_name(const tally *t) {
	return t->name;
}

tally_mode tally_mode_of(const tally *t) {
	return t->mode;
}

void tally_set_mode(tally *t, tally_mode mode) {
	t->mode = mode;
}

size_t tally_add_bytes(tally *t, const uint8_t *data, size_t length) {
	for (size_t i = 0; i < length; i++)
		tally_add(t, data[i]);
	return length;
}

char *tally_describe(const tally *t) {
	char *text = malloc(128);
	if (text)
		snprintf(text, 128, "%s: %lld", t->name, (long long)t->total);
	return text;
}

void tally_string_free(char *text) {
	free(text);
}

bool tally_is_empty(const tally *t) {
	return t->count == 0;
}

static double distance_1d(double a, double b) {
	return a > b ? a - b : b - a;
}

double tally_distance(tally_point a, const tally_point *b) {
	return distance_1d(a.x, b->x) + distance_1d(a.y, b->y);
}

tally_point tally_midpoint(tally_point a, tally_point b) {
	tally_point m = {(a.x + b.x) / 2, (a.y + b.y) / 2};
	return m;
}

unsigned long tally_version(void) {
	return 3;
}

int tally_parse(const char *text, int *out) {
	*out = atoi(text);
	return 0;
}
