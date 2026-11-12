#ifndef COUNTER_ABI_H
#define COUNTER_ABI_H

#include "hxn.h"

#ifdef __cplusplus
extern "C" {
#endif

hxn_status counter_new(int32_t start, hxn_handle **out, hxn_buf *err);
hxn_status counter_add(hxn_handle *self, int32_t amount, int32_t *out, hxn_buf *err);
/* `f` takes an int32_t and returns one: hxn_status (*)(void *env, int32_t value, int32_t *out). */
hxn_status counter_apply(hxn_handle *self, int32_t times, hxn_callback f, int32_t *out, hxn_buf *err);
hxn_status counter_label(hxn_handle *self, hxn_slice name, hxn_buf *out, hxn_buf *err);
hxn_status counter_sum(hxn_handle *self, hxn_slice data, int32_t *out, hxn_buf *err);
hxn_status counter_close(hxn_handle *self, hxn_buf *err);
void counter_release(hxn_handle *self);
int32_t counter_live(void);
hxn_status counter_fail(hxn_buf *err);
void counter_nap(int32_t milliseconds);

/* Futures: one handle type for every result type. The result is taken with
 * the function for its type. */
hxn_status counter_square_later(int32_t value, int32_t milliseconds, hxn_handle **out, hxn_buf *err);
hxn_status counter_greet_later(hxn_slice name, int32_t milliseconds, hxn_handle **out, hxn_buf *err);
hxn_status counter_sum_later(int32_t limit, hxn_handle **out, hxn_buf *err);
hxn_status counter_combined_later(int32_t value, int32_t limit, hxn_handle **out, hxn_buf *err);
hxn_status counter_abandoned(hxn_handle **out, hxn_buf *err);
hxn_status counter_future_state(hxn_handle *self, int32_t *out, hxn_buf *err);
hxn_status counter_future_wait(hxn_handle *self, int32_t milliseconds, int32_t *out, hxn_buf *err);
hxn_status counter_future_take_int(hxn_handle *self, int32_t *out, hxn_buf *err);
hxn_status counter_future_take_text(hxn_handle *self, hxn_buf *out, hxn_buf *err);

/* A future the host settles. resolve writes 1 to `out` if this call settled it. */
hxn_status counter_promise_new(hxn_handle **out, hxn_buf *err);
hxn_status counter_promise_future(hxn_handle *self, hxn_handle **out, hxn_buf *err);
hxn_status counter_promise_doubled(hxn_handle *self, hxn_handle **out, hxn_buf *err);
hxn_status counter_promise_resolve(hxn_handle *self, int32_t value, int32_t *out, hxn_buf *err);

/* Streams: read with wait (0 nothing yet, 1 an item is ready, 2 ended) and
 * the next function for the item's type. */
hxn_status counter_count_up(int32_t limit, int32_t fail, hxn_handle **out, hxn_buf *err);
hxn_status counter_stream_wait(hxn_handle *self, int32_t milliseconds, int32_t *out, hxn_buf *err);
hxn_status counter_stream_next_int(hxn_handle *self, int32_t *out, hxn_buf *err);

/* A stream the host feeds. emit and end write 1 to `out` if the stream
 * accepted the call. */
hxn_status counter_emitter_new(hxn_handle **out, hxn_buf *err);
hxn_status counter_emitter_stream(hxn_handle *self, hxn_handle **out, hxn_buf *err);
hxn_status counter_emitter_emit(hxn_handle *self, int32_t value, int32_t *out, hxn_buf *err);
hxn_status counter_emitter_end(hxn_handle *self, int32_t *out, hxn_buf *err);

/* Signals: a value that changes over time. wait takes the last version seen
 * (negative for none) and returns the current version, or -1 once the value
 * is final. */
hxn_status counter_ramp(int32_t limit, hxn_handle **out, hxn_buf *err);
hxn_status counter_signal_wait(hxn_handle *self, int32_t seen, int32_t milliseconds, int32_t *out, hxn_buf *err);
hxn_status counter_signal_get_int(hxn_handle *self, int32_t *out, hxn_buf *err);

/* A value the host sets and observes as a signal. */
hxn_status counter_gauge_new(int32_t initial, hxn_handle **out, hxn_buf *err);
hxn_status counter_gauge_signal(hxn_handle *self, hxn_handle **out, hxn_buf *err);
hxn_status counter_gauge_set(hxn_handle *self, int32_t value, int32_t *out, hxn_buf *err);

/* The library's single queue of items ready for the host: wait returns 1
 * when something is queued, and next returns the id of the queued item, or
 * -1. The watch calls register a future, stream or signal with the queue and
 * return its id. */
int32_t counter_outbox_wait(int32_t milliseconds);
int32_t counter_outbox_next(void);
hxn_status counter_future_watch(hxn_handle *self, int32_t *out, hxn_buf *err);
hxn_status counter_stream_watch(hxn_handle *self, int32_t *out, hxn_buf *err);
hxn_status counter_signal_watch(hxn_handle *self, int32_t *out, hxn_buf *err);

#ifdef __cplusplus
}
#endif

#endif
