#ifndef COUNTER_ABI_H
#define COUNTER_ABI_H

#include "hxn.h"

#ifdef __cplusplus
extern "C" {
#endif

hxn_status counter_new(int32_t start, hxn_handle **out, hxn_buf *err);
hxn_status counter_add(hxn_handle *self, int32_t amount, int32_t *out, hxn_buf *err);
/* `f` is int32_t in, int32_t out: hxn_status (*)(void *env, int32_t value, int32_t *out). */
hxn_status counter_apply(hxn_handle *self, int32_t times, hxn_callback f, int32_t *out, hxn_buf *err);
hxn_status counter_label(hxn_handle *self, hxn_slice name, hxn_buf *out, hxn_buf *err);
hxn_status counter_sum(hxn_handle *self, hxn_slice data, int32_t *out, hxn_buf *err);
hxn_status counter_close(hxn_handle *self, hxn_buf *err);
void counter_release(hxn_handle *self);
int32_t counter_live(void);
hxn_status counter_fail(hxn_buf *err);
void counter_nap(int32_t milliseconds);

/* Futures: one handle type whatever the result, taken with its own type. */
hxn_status counter_square_later(int32_t value, int32_t milliseconds, hxn_handle **out, hxn_buf *err);
hxn_status counter_greet_later(hxn_slice name, int32_t milliseconds, hxn_handle **out, hxn_buf *err);
hxn_status counter_sum_later(int32_t limit, hxn_handle **out, hxn_buf *err);
hxn_status counter_combined_later(int32_t value, int32_t limit, hxn_handle **out, hxn_buf *err);
hxn_status counter_abandoned(hxn_handle **out, hxn_buf *err);
hxn_status counter_future_state(hxn_handle *self, int32_t *out, hxn_buf *err);
hxn_status counter_future_wait(hxn_handle *self, int32_t milliseconds, int32_t *out, hxn_buf *err);
hxn_status counter_future_take_int(hxn_handle *self, int32_t *out, hxn_buf *err);
hxn_status counter_future_take_text(hxn_handle *self, hxn_buf *out, hxn_buf *err);

/* A future the host settles. `out` of resolve is 1 if this call settled it. */
hxn_status counter_promise_new(hxn_handle **out, hxn_buf *err);
hxn_status counter_promise_future(hxn_handle *self, hxn_handle **out, hxn_buf *err);
hxn_status counter_promise_doubled(hxn_handle *self, hxn_handle **out, hxn_buf *err);
hxn_status counter_promise_resolve(hxn_handle *self, int32_t value, int32_t *out, hxn_buf *err);

/* Streams: read with wait (0 nothing yet, 1 something to take, 2 ended) and
 * a next of the item's own type. */
hxn_status counter_count_up(int32_t limit, int32_t fail, hxn_handle **out, hxn_buf *err);
hxn_status counter_stream_wait(hxn_handle *self, int32_t milliseconds, int32_t *out, hxn_buf *err);
hxn_status counter_stream_next_int(hxn_handle *self, int32_t *out, hxn_buf *err);

/* A stream the host feeds. `out` of emit and end is 1 if it was taken. */
hxn_status counter_emitter_new(hxn_handle **out, hxn_buf *err);
hxn_status counter_emitter_stream(hxn_handle *self, hxn_handle **out, hxn_buf *err);
hxn_status counter_emitter_emit(hxn_handle *self, int32_t value, int32_t *out, hxn_buf *err);
hxn_status counter_emitter_end(hxn_handle *self, int32_t *out, hxn_buf *err);

#ifdef __cplusplus
}
#endif

#endif
