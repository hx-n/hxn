/* The layouts of hxn-abi, for C and C++, and what hxcpp glue needs around
 * them. crates/hxn-abi is the definition; its tests assert these sizes. */
#ifndef HXN_H
#define HXN_H

#include <stddef.h>
#include <stdint.h>

/* Zero is success, negative values are hxn's and positive the library's. */
typedef int32_t hxn_status;

#define HXN_OK 0
#define HXN_ERROR 1
#define HXN_PANIC (-1)
#define HXN_NULL (-2)
#define HXN_INVALID_HANDLE (-3)
#define HXN_CLOSED (-4)
#define HXN_BUSY (-5)
#define HXN_ENCODING (-6)
#define HXN_TOO_LARGE (-7)
#define HXN_PENDING (-8)
#define HXN_CANCELLED (-9)
#define HXN_CALLBACK (-10)
#define HXN_RANGE (-11)
#define HXN_DEPTH (-12)

/* Bytes or UTF-8 text the caller keeps alive for the call. */
typedef struct hxn_slice {
	const uint8_t *ptr;
	size_t len;
} hxn_slice;

typedef struct hxn_slice_mut {
	uint8_t *ptr;
	size_t len;
} hxn_slice_mut;

/* Bytes the library allocated. Copy them, then hxn_buf_free, exactly once. */
typedef struct hxn_buf {
	uint8_t *ptr;
	size_t len;
	size_t cap;
	void (*free)(uint8_t *ptr, size_t len, size_t cap);
} hxn_buf;

#define HXN_BUF_EMPTY { 0, 0, 0, 0 }

static inline void hxn_buf_free(hxn_buf *buf) {
	if (buf->free)
		buf->free(buf->ptr, buf->len, buf->cap);
	buf->ptr = 0;
	buf->len = 0;
	buf->cap = 0;
	buf->free = 0;
}

/* A Rust value. Only the library that made one can read it. */
typedef struct hxn_handle hxn_handle;

/* A host closure. `call` takes `env` first; its real type is the callback's
 * signature, which by convention returns an hxn_status and writes its result
 * through a last `out` parameter. `drop` is null for a callback lent for one
 * call.
 *
 * `call` must return normally. A binding wraps the host's function so that an
 * exception is kept (hxn.Callback.failed) and HXN_CALLBACK returned in its
 * place, then rethrows it after the library has returned. */
typedef struct hxn_callback {
	void *call;
	void *env;
	void (*drop)(void *env);
} hxn_callback;

#if defined(__cplusplus) && defined(HXCPP_H)

#include <stdlib.h>
#include <string.h>

namespace hxn {

/* Copies the buffer into a Haxe String and frees it. */
inline ::String take_string(hxn_buf &buf) {
	::String text = buf.len ? ::String::create((const char *)buf.ptr, (int)buf.len) : HX_CSTRING("");
	hxn_buf_free(&buf);
	return text;
}

/* Copies the buffer into a haxe.io.BytesData and frees it. */
inline Array<unsigned char> take_bytes(hxn_buf &buf) {
	Array<unsigned char> data = Array_obj<unsigned char>::__new((int)buf.len, (int)buf.len);
	if (buf.len)
		memcpy(data->getBase(), buf.ptr, buf.len);
	hxn_buf_free(&buf);
	return data;
}

/* A Haxe String of the UTF-8 text the library lent. */
inline ::String text(hxn_slice slice) {
	return slice.len ? ::String::create((const char *)slice.ptr, (int)slice.len) : HX_CSTRING("");
}

/* A copy of the bytes the library lent, for a haxe.io.Bytes to own. */
inline Array<unsigned char> bytes(hxn_slice slice) {
	Array<unsigned char> data = Array_obj<unsigned char>::__new((int)slice.len, (int)slice.len);
	if (slice.len)
		memcpy(data->getBase(), slice.ptr, slice.len);
	return data;
}

inline void free_text(uint8_t *ptr, size_t, size_t) {
	free(ptr);
}

/* Writes a copy of `text` as UTF-8 for the library to free, which is how a
 * host's function gives text back. False if there is no memory for it. */
inline bool give_text(const ::String &text, hxn_buf *out) {
	hx::strbuf storage;
	int length = 0;
	const char *utf8 = text.utf8_str(&storage, true, &length);
	hxn_buf empty = HXN_BUF_EMPTY;
	*out = empty;
	if (!utf8 || length <= 0)
		return true;
	out->ptr = (uint8_t *)malloc((size_t)length);
	if (!out->ptr)
		return false;
	memcpy(out->ptr, utf8, (size_t)length);
	out->len = (size_t)length;
	out->cap = (size_t)length;
	out->free = free_text;
	return true;
}

/* The same for bytes. */
inline bool give_bytes(const Array<unsigned char> &data, hxn_buf *out) {
	hxn_buf empty = HXN_BUF_EMPTY;
	*out = empty;
	int length = data.mPtr ? data->length : 0;
	if (length <= 0)
		return true;
	out->ptr = (uint8_t *)malloc((size_t)length);
	if (!out->ptr)
		return false;
	memcpy(out->ptr, data->getBase(), (size_t)length);
	out->len = (size_t)length;
	out->cap = (size_t)length;
	out->free = free_text;
	return true;
}

/* `text` as UTF-8, valid while `text` and `storage` both are. */
inline hxn_slice slice(const ::String &text, hx::strbuf &storage) {
	int length = 0;
	const char *utf8 = text.utf8_str(&storage, true, &length);
	hxn_slice out = { (const uint8_t *)utf8, utf8 ? (size_t)length : 0 };
	return out;
}

/* Valid while `data` is, and is not resized. */
inline hxn_slice slice(const Array<unsigned char> &data) {
	hxn_slice out = { 0, 0 };
	if (data.mPtr) {
		out.ptr = (const uint8_t *)data->getBase();
		out.len = (size_t)data->length;
	}
	return out;
}

/* Throws a failed call's message to Haxe. Called after the library has
 * returned, so the exception crosses no Rust frame. */
inline void check(hxn_status status, hxn_buf &err) {
	if (status == HXN_OK)
		return;
	::String message = err.len ? take_string(err) : HX_CSTRING("native call failed");
	hx::Throw(message);
}

/* Lets the collector run without this thread, for a call that waits. Nothing
 * in its scope may touch the Haxe heap. */
typedef hx::AutoGCFreeZone Blocking;

}

#endif

#endif
