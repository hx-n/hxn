class Main {
	// sqlite3's result code for a row; a `#define`, which is not imported.
	static inline final ROW = 100;
	static inline final INSERT = 18;

	static function main() {
		var db = new Sqlite3(":memory:");
		// Hooks sqlite3 keeps are streams, which end when the database is
		// closed.
		var updates = db.updateHook();
		var commits = db.commitHook();
		run(db, "CREATE TABLE fruit (name TEXT, count INTEGER)");
		run(db, "INSERT INTO fruit VALUES ('apple', 3), ('pear', 5), ('fig', 0)");
		var query = db.prepareV2("SELECT name, count FROM fruit WHERE count > 0 ORDER BY name", -1);
		Sys.println('${query.columnCount()} columns: ${query.columnName(0)}, ${query.columnName(1)}');
		while (query.step() == ROW)
			Sys.println('${query.columnText(0)}: ${query.columnInt(1)}');
		query.close();
		try {
			db.prepareV2("SELECT name FROM nowhere", -1);
		} catch (error:haxe.Exception) {
			Sys.println('refused: ${error.message} (${db.errmsg()})');
		}
		db.close();
		for (update in updates)
			Sys.println('updated: ${update.value0 == INSERT} ${update.value1}.${update.value2} row ${haxe.Int64.toStr(update.value3)}');
		Sys.println('${[for (commit in commits) commit].length} commits');
		hxn.Loop.exit(0);
	}

	static function run(db:Sqlite3, sql:String) {
		var statement = db.prepareV2(sql, -1);
		statement.step();
		statement.close();
	}
}
