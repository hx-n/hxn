/**
	Two futures settled by the host after `main` returns, the second
	requested from the first's callback, and a third settled by work on
	threads where the build has them.
**/
class Main {
	static function main() {
		Timer.after(20).then(ms -> {
			Sys.println('first after $ms ms');
			Timer.after(5).then(ms -> {
				Sys.println('then after $ms ms');
				Timer.chunks(3).then(n -> Sys.println('$n chunks of work done'));
			});
		});
		Sys.println("waiting");
	}
}
