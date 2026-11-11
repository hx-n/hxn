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

}

#endif
