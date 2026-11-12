//! Results that arrive more than once: a queue the library fills from any
//! thread and the host drains on one of its own threads.
//!
//! A library reports events this way. It never calls a host function to do
//! so: it sends, and the host takes each item and passes it to whatever it
//! chooses. So no host object is held here and has to be kept alive, and the
//! sending thread need not be registered with the host's runtime.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use hxn_abi::{Failure, Fault, Status};

use crate::outbox::{self, Tag};
use crate::{THREADS, local};

/// The state of a stream. The discriminants are the values a host receives.
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Flow {
    /// Nothing to take yet.
    Waiting = 0,
    /// An item, or the failure that ended the stream, is ready to take.
    Ready = 1,
    /// Every item was taken and the stream is closed.
    Ended = 2,
}

enum End {
    Open,
    Closed,
    // Taken as the final entry of the stream, after the items sent before it.
    Failed(Failure),
}

struct Queue<T> {
    items: VecDeque<T>,
    end: End,
    // Set when the read side is dropped, which tells a sender to stop.
    unread: bool,
}

impl<T> Queue<T> {
    fn flow(&self) -> Flow {
        match (&self.end, self.items.is_empty()) {
            (_, false) | (End::Failed(_), true) => Flow::Ready,
            (End::Open, true) => Flow::Waiting,
            (End::Closed, true) => Flow::Ended,
        }
    }
}

struct Shared<T> {
    queue: Mutex<Queue<T>>,
    changed: Condvar,
    tag: Arc<Tag>,
}

impl<T> Shared<T> {
    fn lock(&self) -> MutexGuard<'_, Queue<T>> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    // Ends the stream unless it has ended already.
    fn end(&self, end: End) -> bool {
        let mut queue = self.lock();
        if !matches!(queue.end, End::Open) || queue.unread {
            return false;
        }
        queue.end = end;
        drop(queue);
        self.changed.notify_all();
        outbox::publish(&self.tag);
        true
    }
}

// Shared by every clone of a sender. Dropping the last clone closes the
// stream.
struct Owner<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Drop for Owner<T> {
    fn drop(&mut self) {
        self.shared.end(End::Closed);
    }
}

/// The write side of a stream. All clones send to the same stream, which ends
/// when the last clone is dropped.
pub struct Sender<T> {
    owner: Arc<Owner<T>>,
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            owner: self.owner.clone(),
        }
    }
}

impl<T> Sender<T> {
    /// Adds `item` to the stream. Returns `false` if the stream has ended or
    /// its reader was dropped, which tells the caller to stop.
    pub fn send(&self, item: T) -> bool {
        let shared = &self.owner.shared;
        let mut queue = shared.lock();
        if !matches!(queue.end, End::Open) || queue.unread {
            // Unlocked before `item` is dropped, because its `Drop` is
            // library code.
            drop(queue);
            return false;
        }
        queue.items.push_back(item);
        drop(queue);
        shared.changed.notify_all();
        outbox::publish(&shared.tag);
        true
    }

    /// Ends the stream with a failure, which the reader receives after the
    /// items already sent. Returns `false` if the stream had already ended.
    pub fn fail(&self, failure: impl Into<Failure>) -> bool {
        self.owner.shared.end(End::Failed(failure.into()))
    }

    /// Ends the stream. Dropping every sender does the same.
    pub fn close(&self) -> bool {
        self.owner.shared.end(End::Closed)
    }
}

/// The read side of a stream.
pub struct Stream<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Stream<T> {
    pub fn flow(&self) -> Flow {
        self.shared.lock().flow()
    }

    /// Blocks until there is something to take, the stream has ended, or
    /// `timeout` has elapsed, and returns its state at that point. `None`
    /// waits indefinitely. Without threads it runs queued work for a short
    /// time slice instead of blocking.
    pub fn wait(&self, timeout: Option<Duration>) -> Flow {
        if !THREADS {
            return self.wait_driving(timeout);
        }
        let deadline = timeout.and_then(|timeout| Instant::now().checked_add(timeout));
        let mut queue = self.shared.lock();
        loop {
            let flow = queue.flow();
            if flow != Flow::Waiting {
                return flow;
            }
            queue = match deadline {
                None => self
                    .shared
                    .changed
                    .wait(queue)
                    .unwrap_or_else(PoisonError::into_inner),
                Some(deadline) => {
                    let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                        return Flow::Waiting;
                    };
                    if left.is_zero() {
                        return Flow::Waiting;
                    }
                    self.shared
                        .changed
                        .wait_timeout(queue, left)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
            };
        }
    }

    pub(crate) fn wait_driving(&self, timeout: Option<Duration>) -> Flow {
        local::drive(timeout, || self.flow() != Flow::Waiting);
        self.flow()
    }

    /// Takes the next item. Returns `None` once the stream has ended, or the
    /// failure that ended it, and [`Status::PENDING`] while there is nothing
    /// to take yet.
    pub fn take(&self) -> Result<Option<T>, Failure> {
        let mut queue = self.shared.lock();
        if let Some(item) = queue.items.pop_front() {
            return Ok(Some(item));
        }
        match std::mem::replace(&mut queue.end, End::Closed) {
            End::Open => {
                queue.end = End::Open;
                Err(Failure::new(Status::PENDING, "stream has nothing yet"))
            }
            End::Closed => Ok(None),
            End::Failed(failure) => Err(failure),
        }
    }
}

impl<T> Drop for Stream<T> {
    fn drop(&mut self) {
        let mut queue = self.shared.lock();
        queue.unread = true;
        let unread = std::mem::take(&mut queue.items);
        drop(queue);
        // Dropped outside the lock, like every other library value.
        drop(unread);
    }
}

/// Creates a stream and its sender.
pub fn channel<T>() -> (Sender<T>, Stream<T>) {
    let shared = Arc::new(Shared {
        queue: Mutex::new(Queue {
            items: VecDeque::new(),
            end: End::Open,
            unread: false,
        }),
        changed: Condvar::new(),
        tag: Tag::new(),
    });
    let owner = Arc::new(Owner {
        shared: shared.clone(),
    });
    (Sender { owner }, Stream { shared })
}

trait Erased: Send + Sync {
    fn flow(&self) -> Flow;
    fn wait(&self, timeout: Option<Duration>) -> Flow;
    fn watch(&self) -> i32;
    fn as_any(&self) -> &dyn std::any::Any;
}

impl<T: Send + 'static> Erased for Stream<T> {
    fn flow(&self) -> Flow {
        Stream::flow(self)
    }

    fn wait(&self, timeout: Option<Duration>) -> Flow {
        Stream::wait(self, timeout)
    }

    fn watch(&self) -> i32 {
        self.shared.tag.watch()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A type-erased stream, which is what a host holds. Only
/// [`next`](Self::next) needs the concrete type.
pub struct AnyStream(Box<dyn Erased>);

impl<T: Send + 'static> From<Stream<T>> for AnyStream {
    fn from(stream: Stream<T>) -> Self {
        Self(Box::new(stream))
    }
}

impl AnyStream {
    pub fn flow(&self) -> Flow {
        self.0.flow()
    }

    pub fn wait(&self, timeout: Option<Duration>) -> Flow {
        self.0.wait(timeout)
    }

    /// Registers the stream to be posted to the [`outbox`](crate::outbox)
    /// when there is something to take, and returns the id it is posted
    /// under.
    pub fn watch(&self) -> i32 {
        self.0.watch()
    }

    /// Like [`wait`](Self::wait), with the timeout in the form a host passes
    /// it: milliseconds, or negative for no limit.
    pub fn wait_millis(&self, milliseconds: i32) -> Flow {
        self.wait(u64::try_from(milliseconds).ok().map(Duration::from_millis))
    }

    /// Takes the next item, for a host that has seen [`Flow::Ready`]. The
    /// failure that ended the stream is returned in order after the items.
    /// Reading past the end returns [`Fault::Closed`], and reading with the
    /// wrong type returns [`Fault::InvalidHandle`].
    pub fn next<T: Send + 'static>(&self) -> Result<T, Failure> {
        match self.0.as_any().downcast_ref::<Stream<T>>() {
            Some(stream) => stream.take()?.ok_or_else(|| Fault::Closed.into()),
            None => Err(Fault::InvalidHandle.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_come_out_in_order_and_then_the_end() {
        let (sender, stream) = channel();
        assert_eq!(stream.flow(), Flow::Waiting);
        assert_eq!(stream.take().unwrap_err().status, Status::PENDING);
        assert!(sender.send(1));
        assert!(sender.send(2));
        assert_eq!(stream.flow(), Flow::Ready);
        assert!(sender.close());
        assert!(!sender.send(3));
        assert_eq!(stream.take(), Ok(Some(1)));
        assert_eq!(stream.take(), Ok(Some(2)));
        assert_eq!(stream.flow(), Flow::Ended);
        assert_eq!(stream.take(), Ok(None));
    }

    #[test]
    fn a_failure_is_met_after_the_items_before_it() {
        let (sender, stream) = channel();
        sender.send("kept");
        assert!(sender.fail(Failure::new(Status(7), "its own code")));
        assert!(!sender.fail("again"));
        assert_eq!(stream.take(), Ok(Some("kept")));
        assert_eq!(stream.flow(), Flow::Ready);
        assert_eq!(stream.take(), Err(Failure::new(Status(7), "its own code")));
        assert_eq!(stream.flow(), Flow::Ended);
        assert_eq!(stream.take(), Ok(None));
    }

    #[test]
    fn dropping_every_sender_ends_the_stream() {
        let (sender, stream) = channel();
        let other = sender.clone();
        other.send(1);
        drop(sender);
        assert_eq!(stream.flow(), Flow::Ready);
        drop(other);
        assert_eq!(stream.take(), Ok(Some(1)));
        assert_eq!(stream.wait(None), Flow::Ended);
    }

    #[test]
    fn a_sender_learns_its_reader_is_gone() {
        let (sender, stream) = channel();
        let unread = Arc::new(());
        assert!(sender.send(unread.clone()));
        drop(stream);
        assert_eq!(Arc::strong_count(&unread), 1);
        assert!(!sender.send(unread.clone()));
        assert!(!sender.close());
    }

    #[test]
    fn wait_returns_at_the_timeout_or_the_next_item() {
        let (sender, stream) = channel();
        assert_eq!(stream.wait(Some(Duration::ZERO)), Flow::Waiting);
        assert_eq!(stream.wait(Some(Duration::from_millis(5))), Flow::Waiting);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for item in 0..100 {
                    sender.send(item);
                }
                sender.close();
            });
            let mut seen = Vec::new();
            while stream.wait(None) == Flow::Ready {
                seen.push(stream.take().unwrap().unwrap());
            }
            assert_eq!(seen, (0..100).collect::<Vec<_>>());
        });
    }

    #[test]
    fn an_erased_stream_gives_only_its_own_type() {
        let (sender, stream) = channel::<String>();
        let erased = AnyStream::from(stream);
        assert_eq!(erased.wait_millis(0), Flow::Waiting);
        sender.send(String::from("typed"));
        drop(sender);
        assert_eq!(erased.wait_millis(-1), Flow::Ready);
        assert_eq!(
            erased.next::<i32>().unwrap_err().status,
            Status::INVALID_HANDLE
        );
        assert_eq!(erased.next::<String>().as_deref(), Ok("typed"));
        assert_eq!(erased.flow(), Flow::Ended);
        assert_eq!(erased.next::<String>().unwrap_err().status, Status::CLOSED);
    }
}
