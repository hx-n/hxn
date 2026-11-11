#ifndef COUNTER_ABI_H
#define COUNTER_ABI_H

#include "hxn.h"

#ifdef __cplusplus
extern "C" {
#endif

hxn_status counter_new(int32_t start, hxn_handle **out, hxn_buf *err);
hxn_status counter_add(hxn_handle *self, int32_t amount, int32_t *out, hxn_buf *err);
hxn_status counter_label(hxn_handle *self, hxn_slice name, hxn_buf *out, hxn_buf *err);
hxn_status counter_sum(hxn_handle *self, hxn_slice data, int32_t *out, hxn_buf *err);
hxn_status counter_close(hxn_handle *self, hxn_buf *err);
void counter_release(hxn_handle *self);
int32_t counter_live(void);
hxn_status counter_fail(hxn_buf *err);
void counter_nap(int32_t milliseconds);

#ifdef __cplusplus
}
#endif

#endif
