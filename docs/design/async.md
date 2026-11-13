# Futures, streams and scheduling

[Design](../design.md) · [Conventions](../conventions.md)

Results stay in Rust-owned storage until Haxe retrieves them. Producer threads
never touch the runtime or its heap.

| Rust constructor | Result | Haxe |
|---|---|---|
| `pending()` | One result; settles once. Dropping every producer cancels it and wakes waiters. | `Future.await`, `then` |
| `channel()` | Every item in order; ends after all senders drop or with a failure after earlier items. | `Stream` iteration, `listen` |
| `signal()` | Latest value. | `Signal`, `Reactive.follow` |

Repeated future settlement returns false. `send` and `set` return false when
the reader is gone.

Futures and streams carry scalars, text, bytes, data, objects, object lists,
optional objects or `()` (`hxn.Unit`). Signals carry scalars, text and
cloneable data. Taking objects transfers ownership to Haxe; object lists
use the [completed-stream protocol](handles.md#ownership).

`async fn` becomes a future started with `spawn`. It owns copied text/bytes
and shared object references; the receiver is `self` or shared `&self`.
Call-only borrows, cells and Haxe functions are rejected. `Result` errors
fail the future; see [typed failures](boundary.md#failures).

## Scheduling

| Entry point | With threads | Without threads |
|---|---|---|
| `spawn(async { … })` | Worker pool; polls only when woken. | Calling thread's queue. |
| `stepped(\|\| …)` | Worker pool. | Calling thread's queue. |
| `spawn_blocking(\|\| …)` | Dedicated thread. | Runs before returning. |

The schedulers use `krio-parallel` and `krio-runtime`. `spawn` and `stepped`
run their first step before returning. Waiting futures sleep until woken.
Without threads, waits advance a bounded queue slice so the host can yield.

Use `stepped` for computation: `Poll::Pending` requests another immediate
step. Use `spawn` for work that waits. Tasks never call Haxe functions.

## Delivery

Each library has one outbox and one `hxn.Inbox`. Watched result ids are
queued once until read; unwatched results add no entries. The inbox takes
all entries, then runs watchers in production order as one batch.

`hxn.Loop` delivers through the Haxe event loop on the thread that first
requested delivery. For an application-owned loop, set `Loop.manual` and
call `Loop.sync()`; automatic delivery no longer keeps the event loop alive.

`hxn_future::together(|| { … })` batches related signal writes. Scopes on
different threads serialize; host reads wait until the scope ends. Rust
reads never wait. Effects run after the complete batch has been read.

See [reactivity](reactivity.md) and [thread rules](../conventions/threads.md).
