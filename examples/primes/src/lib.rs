//! Work that takes a while, done off the caller's thread: one result that
//! arrives later, and results that arrive one at a time.

use hxn_future::{Failure, Future, Stream, channel, spawn_blocking};

/// Finds prime numbers without holding up the caller.
#[hxn::interface]
pub struct Primes;

fn is_prime(number: i32) -> bool {
    let mut divisors = (2..).take_while(|d| *d <= number / d);
    number >= 2 && divisors.all(|d| number % d != 0)
}

#[hxn::methods]
impl Primes {
    /// How many primes are below `limit`, counted on another thread. Fails
    /// at once for a negative limit.
    pub fn count(limit: i32) -> Result<Future<i32>, &'static str> {
        if limit < 0 {
            return Err("a negative limit");
        }
        Ok(spawn_blocking(move || {
            let count = (2..limit).filter(|number| is_prime(*number)).count();
            Ok::<_, Failure>(count as i32)
        }))
    }

    /// The primes below `limit`, each sent as it is found.
    pub fn below(limit: i32) -> Stream<i32> {
        let (sender, stream) = channel();
        // Nobody waits for this work to return: the stream is what is read,
        // and it ends when the sender is dropped.
        drop(spawn_blocking(move || {
            for number in 2..limit {
                // `send` is false once the reader has gone.
                if is_prime(number) && !sender.send(number) {
                    break;
                }
            }
            Ok::<_, Failure>(())
        }));
        stream
    }
}
