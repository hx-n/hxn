class Main {
	static function main() {
		// A future. `await` blocks this thread until the result is in.
		var count = Primes.count(100000).await();
		Sys.println('$count primes below 100000');

		// A stream is read an item at a time, as the library finds them.
		var found = [for (prime in Primes.below(30)) prime];
		Sys.println(found.join(" "));

		// Without blocking: `then` is called from the event loop, after
		// `main` has returned.
		Primes.count(1000).then(count -> Sys.println('$count primes below 1000'));
		Sys.println("counting");
	}
}
