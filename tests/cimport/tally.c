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
	int32_t recent[16];
	tally_listener listener;
	void *listener_user;
	char *label;
	tally_destructor label_destructor;
};

static void release_label(tally *t) {
	if (t->label && t->label_destructor)
		t->label_destructor(t->label);
	t->label = NULL;
	t->label_destructor = NULL;
}

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
	if (t)
		release_label(t);
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
	t->recent[t->count % 16] = amount;
	t->count += 1;
	if (t->listener)
		t->listener(t->listener_user, t->name, amount, t->total);
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

double tally_mean(const double *values, size_t count) {
	double sum = 0;
	for (size_t i = 0; i < count; i++)
		sum += values[i];
	return count ? sum / (double)count : 0;
}

void tally_recent(const tally *t, int32_t *amounts, size_t count) {
	for (size_t i = 0; i < count; i++) {
		/* The amount `count - i` back from the latest. */
		size_t back = count - i;
		int64_t at = t->count - (int64_t)back;
		amounts[i] = (at >= 0 && back <= 16) ? t->recent[at % 16] : 0;
	}
}

int32_t tally_checksum(const uint8_t *data, int32_t length) {
	int32_t sum = 0;
	for (int32_t i = 0; i < length; i++)
		sum += data[i];
	return sum;
}

int tally_add_blob(tally *t, const void *data, int length, tally_destructor destructor) {
	if (length < 0)
		return -1;
	tally_add_bytes(t, data, (size_t)length);
	if (destructor && destructor != TALLY_TRANSIENT)
		destructor((void *)data);
	return 0;
}

void tally_set_label(tally *t, const char *label, tally_destructor destructor) {
	release_label(t);
	if (destructor == TALLY_TRANSIENT) {
		size_t length = strlen(label) + 1;
		t->label = malloc(length);
		if (t->label)
			memcpy(t->label, label, length);
		t->label_destructor = free;
	} else {
		t->label = (char *)label;
		t->label_destructor = destructor;
	}
}

const char *tally_label(const tally *t) {
	return t->label ? t->label : "";
}

size_t tally_write_name(const tally *t, char *buffer, size_t size) {
	size_t length = strlen(t->name);
	if (length > size)
		length = size;
	memcpy(buffer, t->name, length);
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
	char *end = NULL;
	long value = strtol(text, &end, 10);
	if (!*text || *end || value < INT_MIN || value > INT_MAX)
		return -1;
	*out = (int)value;
	return 0;
}

int tally_copy(const tally *t, const char *name, tally **out) {
	tally *copy = tally_new(name, t->mode);
	if (!copy)
		return -1;
	copy->total = t->total;
	copy->count = t->count;
	*out = copy;
	return 0;
}

int tally_each(const tally *t, tally_visitor visit, void *user) {
	int64_t first = t->count > 16 ? t->count - 16 : 0;
	int visited = 0;
	for (int64_t i = first; i < t->count; i++) {
		visited += 1;
		if (visit(user, t->recent[i % 16]))
			break;
	}
	return visited;
}

void tally_report(const tally *t, tally_reporter report, void *user) {
	char line[64];
	snprintf(line, sizeof line, "total %lld", (long long)t->total);
	report(line, user);
	snprintf(line, sizeof line, "count %lld", (long long)t->count);
	report(line, user);
}

void tally_origin(tally_point *out) {
	out->x = 0;
	out->y = 0;
}

void *tally_listen(tally *t, tally_listener listener, void *user) {
	void *replaced = t->listener_user;
	t->listener = listener;
	t->listener_user = user;
	return replaced;
}
