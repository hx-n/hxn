/**
	Two futures settled by the host after `main` returns, the second
	requested from the first's callback.
**/
class Main {
	static function main() {
		Timer.after(20).then(ms -> {
			Sys.println('first after $ms ms');
			Timer.after(5).then(ms -> Sys.println('then after $ms ms'));
		});
		Sys.println("waiting");
	}
}
