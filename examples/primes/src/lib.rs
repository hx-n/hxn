//! Long-running work done off the caller's thread: a future for a single
//! result, and a stream for results produced one at a time.

use hxn_future::{Failure, Future, Stream, channel, spawn_blocking};

/// Finds prime numbers without blocking the caller.
#[hxn::interface]
pub struct Primes;

fn is_prime(number: i32) -> bool {
    let mut divisors = (2..).take_while(|d| *d <= number / d);
    number >= 2 && divisors.all(|d| number % d != 0)
}

#[hxn::methods]
impl Primes {
    /// Counts the primes below `limit` on another thread. Returns an error
    /// immediately for a negative limit.
    pub fn count(limit: i32) -> Result<Future<i32>, &'static str> {
        if limit < 0 {
            return Err("a negative limit");
        }
        Ok(spawn_blocking(move || {
            let count = (2..limit).filter(|number| is_prime(*number)).count();
            Ok::<_, Failure>(count as i32)
        }))
    }

    /// Streams the primes below `limit`, sending each one as it is found.
    pub fn below(limit: i32) -> Stream<i32> {
        let (sender, stream) = channel();
        // The task's future is dropped because nothing awaits it. The caller
        // reads the stream, which ends when the sender is dropped.
        drop(spawn_blocking(move || {
            for number in 2..limit {
                // `send` returns false once the stream has no reader.
                if is_prime(number) && !sender.send(number) {
                    break;
                }
            }
            Ok::<_, Failure>(())
        }));
        stream
    }
}
