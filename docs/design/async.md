# Futures, streams and scheduling

[Design](../design.md) · [Conventions](../conventions.md)

## Futures, streams and signals

`hxn-future` provides three types of asynchronous result, each with a Haxe
class.

| Result | Rust | Haxe |
|---|---|---|
| one result, later | `pending()` returns a `Completer` and a `Future` | `hxn.Future`: `await()`, `then()` |
| many results, in order | `channel()` returns a `Sender` and a `Stream` | `hxn.Stream`: an iterator, `listen()` |
| a value that changes | `signal()` returns a `Source` and a `Signal` | `hxn.Signal`, through `Reactive.follow` |

Futures, streams and signals store their results in Rust until a Haxe thread
retrieves them. Producer threads settle futures, send stream items and update
signals through Rust-owned storage. They use no runtime objects and need no
collector registration. The Haxe thread retrieves the values and runs their
registered handlers. This delivery protocol also works on wasm.

A future is settled once; later attempts to settle it return `false`. If every
producer is dropped, the future is rejected as cancelled and wakes its
waiters. A stream ends when every sender is dropped, or with an error that the
reader receives after the items sent before it. A sender or source detects
that its reader is gone when `send` or `set` returns `false`.

Use a stream when consumers need every item in order, and a signal when they
need the latest state.

A function exposes one by returning `Future<T>`, `Stream<T>` or `Signal<T>`,
and the Haxe method returns the class of the same name. The generated adapter
returns each as a type-erased handle. All futures in a library share one set
of wait/watch exports, with a separate take export for each value type. A
future or stream can carry a number, a bool, text, bytes, data or an object;
bytes and objects taken from one are owned by the Haxe side. It can also carry
`()`, which Haxe sees as `hxn.Unit`, to signal completion or an event with no
payload. A signal carries a number, a bool, text or data, but not a
single-owner value, because it is read repeatedly. Data is serialized when a
Haxe thread takes it, by the take function generated for that type; the
library's future holds the library's own Rust type.

A future or stream can also carry a list of objects, `Vec<T>` or
`Vec<Arc<T>>` of a shared interface. Haxe receives `Array<T>`. The Rust
result keeps the vector until it is taken; the take call transfers its
values into an ended stream, and the Haxe wrapper drains that stream into
the array before delivering it. The element take exports are generated even
when no function returns a stream of individual objects. Until an element
is taken it remains owned by Rust, so dropping an untaken result releases
its values without creating host objects.

An `async fn` is exposed as a function that returns a future of its return
value; the error of a `Result` fails the future, as a typed exception if the
error type is `#[hxn::data]`.
The adapter starts it with `spawn`. The work runs after the starting call has
returned, so it must own its parameters: text and bytes are copied, a shared
object is kept by its own `Arc` reference, and the receiver is `self` or the
`&self` of a shared interface. Anything only borrowed for the call is
rejected: a borrowed object that is not shared, a cell, a Haxe function.

## Running work

| Entry point | For | With threads | Without |
|---|---|---|---|
| `spawn(async { … })` | async work | a worker pool; polled only when woken | the calling thread's queue |
| `stepped(\|\| …)` | work done in steps | a worker pool | the calling thread's queue |
| `spawn_blocking(\|\| …)` | a blocking closure | a dedicated thread | runs before returning |

Each piece of work is a `krio` task. With threads it runs on a
`krio-parallel` cluster of a few workers per library, started on first use;
idle workers sleep. Without threads it is queued on the calling thread
(`krio-runtime`) and advanced by whichever caller waits: a wait runs the queue
for a short time slice and returns whether or not the result is ready, so the
host can yield and poll again.

`spawn` and `stepped` run the first step before returning, so work that
completes immediately does not wait for a worker or a waiter.

Settling an `hxn_future::Future` wakes the task awaiting it. Both schedulers
suspend waiting tasks until a wakeup, using no CPU while they wait.

`stepped` is for work that is all computation, split so a single thread can
interleave it with other work: `Poll::Pending` from a step means there is
more to do, and the next step runs at once. Work that waits on something is
an `async` block given to `spawn`. A task never calls a Haxe function; what
it produces reaches Haxe as the future's, stream's or signal's value.

## Delivery

Each library uses one queue to notify Haxe of changes to watched results.

- Rust queues a watched future, stream or signal's identifier when its
  result changes. Each identifier is queued at most once until Haxe reads
  it; unwatched results produce no queue entries.
- `hxn.Inbox`, one per library, drains the queue and calls each entry's
  watchers in production order, as one batch. `Future.then`, `Stream.listen` and `Reactive.follow` register
  watchers.
- `hxn.Loop` schedules delivery. By default it runs from
  the Haxe event loop of the thread that first requested delivery. An app
  with its own main loop sets `Loop.manual` and calls `Loop.sync()` where it
  wants deliveries, for instance once per frame; the Haxe event loop is then
  neither used nor kept alive.

A single queue preserves order across futures, streams and signals, lets the
app choose when delivery happens, and needs only one wait for all of them.

Wrap related signal updates in `hxn_future::together(|| { … })` to deliver
them as one change. The entries the scope's changes would add are held back
until it returns and then added at once, and the host does not read a signal's
value while any scope is running. An inbox takes every entry before it runs
effects, so effects never see one of those values changed and another not.
Scopes on different threads run one at a time, and the library's own reads of
a signal never wait: only the host's do.

See also: [Reactivity](reactivity.md), [Threads and delivery](../conventions/threads.md).
