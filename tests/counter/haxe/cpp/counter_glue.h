/* What hxcpp calls for the counter library. Each function returns before its
 * failure is thrown, and frees what the library handed it. */
#ifndef COUNTER_GLUE_H
#define COUNTER_GLUE_H

#include "counter_abi.h"

namespace counter_glue {

inline void *make(int start) {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_new(start, &out, &err), err);
	return out;
}

inline int add(void *self, int amount) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_add((hxn_handle *)self, amount, &out, &err), err);
	return out;
}

// What the library calls a function of Int->Null<Int> through. The binding's
// wrapper returns null when the user's function threw.
inline hxn_status int_to_int(void *env, int value, int *out) {
	Dynamic result = Dynamic((hx::Object *)env)(value);
	if (result == null())
		return HXN_CALLBACK;
	*out = (int)result;
	return HXN_OK;
}

inline int apply(void *self, int times, Dynamic f) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	// Lent: `f` is on this stack for the call, so the library drops nothing.
	hxn_callback callback = { (void *)int_to_int, f.mPtr, 0 };
	hxn::check(counter_apply((hxn_handle *)self, times, callback, &out, &err), err);
	return out;
}

inline ::String label(void *self, ::String name) {
	hx::strbuf storage;
	hxn_buf out = HXN_BUF_EMPTY;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_label((hxn_handle *)self, hxn::slice(name, storage), &out, &err), err);
	return hxn::take_string(out);
}

inline int sum(void *self, Array<unsigned char> data) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_sum((hxn_handle *)self, hxn::slice(data), &out, &err), err);
	return out;
}

inline void close(void *self) {
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_close((hxn_handle *)self, &err), err);
}

inline void fail() {
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_fail(&err), err);
}

inline void nap(int milliseconds) {
	hxn::Blocking zone;
	counter_nap(milliseconds);
}

inline void *square_later(int value, int milliseconds) {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_square_later(value, milliseconds, &out, &err), err);
	return out;
}

inline void *greet_later(::String name, int milliseconds) {
	hx::strbuf storage;
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_greet_later(hxn::slice(name, storage), milliseconds, &out, &err), err);
	return out;
}

inline void *sum_later(int limit) {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_sum_later(limit, &out, &err), err);
	return out;
}

inline void *combined_later(int value, int limit) {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_combined_later(value, limit, &out, &err), err);
	return out;
}

inline void *abandoned() {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_abandoned(&out, &err), err);
	return out;
}

inline int future_state(void *self) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_future_state((hxn_handle *)self, &out, &err), err);
	return out;
}

inline int future_wait(void *self, int milliseconds) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn_status status;
	{
		// The collector may run while this thread waits; it is back before the throw.
		hxn::Blocking zone;
		status = counter_future_wait((hxn_handle *)self, milliseconds, &out, &err);
	}
	hxn::check(status, err);
	return out;
}

inline int future_take_int(void *self) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_future_take_int((hxn_handle *)self, &out, &err), err);
	return out;
}

inline ::String future_take_text(void *self) {
	hxn_buf out = HXN_BUF_EMPTY;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_future_take_text((hxn_handle *)self, &out, &err), err);
	return hxn::take_string(out);
}

inline void *promise_new() {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_promise_new(&out, &err), err);
	return out;
}

inline void *promise_future(void *self) {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_promise_future((hxn_handle *)self, &out, &err), err);
	return out;
}

inline void *promise_doubled(void *self) {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_promise_doubled((hxn_handle *)self, &out, &err), err);
	return out;
}

inline bool promise_resolve(void *self, int value) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_promise_resolve((hxn_handle *)self, value, &out, &err), err);
	return out != 0;
}

inline void *count_up(int limit, bool fail) {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_count_up(limit, fail ? 1 : 0, &out, &err), err);
	return out;
}

inline int stream_wait(void *self, int milliseconds) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn_status status;
	{
		// The collector may run while this thread waits; it is back before the throw.
		hxn::Blocking zone;
		status = counter_stream_wait((hxn_handle *)self, milliseconds, &out, &err);
	}
	hxn::check(status, err);
	return out;
}

inline int stream_next_int(void *self) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_stream_next_int((hxn_handle *)self, &out, &err), err);
	return out;
}

inline void *emitter_new() {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_emitter_new(&out, &err), err);
	return out;
}

inline void *emitter_stream(void *self) {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_emitter_stream((hxn_handle *)self, &out, &err), err);
	return out;
}

inline bool emitter_emit(void *self, int value) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_emitter_emit((hxn_handle *)self, value, &out, &err), err);
	return out != 0;
}

inline bool emitter_end(void *self) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_emitter_end((hxn_handle *)self, &out, &err), err);
	return out != 0;
}

inline void *ramp(int limit) {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_ramp(limit, &out, &err), err);
	return out;
}

inline int signal_wait(void *self, int seen, int milliseconds) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn_status status;
	{
		// The collector may run while this thread waits; it is back before the throw.
		hxn::Blocking zone;
		status = counter_signal_wait((hxn_handle *)self, seen, milliseconds, &out, &err);
	}
	hxn::check(status, err);
	return out;
}

inline int signal_get_int(void *self) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_signal_get_int((hxn_handle *)self, &out, &err), err);
	return out;
}

inline void *gauge_new(int initial) {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_gauge_new(initial, &out, &err), err);
	return out;
}

inline void *gauge_signal(void *self) {
	hxn_handle *out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_gauge_signal((hxn_handle *)self, &out, &err), err);
	return out;
}

inline bool gauge_set(void *self, int value) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_gauge_set((hxn_handle *)self, value, &out, &err), err);
	return out != 0;
}

inline bool outbox_wait(int milliseconds) {
	// The collector may run while this thread waits.
	hxn::Blocking zone;
	return counter_outbox_wait(milliseconds) != 0;
}

inline int outbox_next() {
	return counter_outbox_next();
}

inline int future_watch(void *self) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_future_watch((hxn_handle *)self, &out, &err), err);
	return out;
}

inline int stream_watch(void *self) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_stream_watch((hxn_handle *)self, &out, &err), err);
	return out;
}

inline int signal_watch(void *self) {
	int out = 0;
	hxn_buf err = HXN_BUF_EMPTY;
	hxn::check(counter_signal_watch((hxn_handle *)self, &out, &err), err);
	return out;
}

}

#endif
