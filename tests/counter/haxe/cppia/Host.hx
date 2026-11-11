/**
	The hxcpp program a CPPIA script of the fixture runs in. It holds the
	library's generated classes and hxn's own, and the script is passed to it
	by name.
**/
// hxcpp's own zlib, which a host of scripts links, takes `fdopen` for missing
// on a Mac unless it is already a macro.
@:buildXml('
<compiler id="darwin" exe="clang++" if="macos">
	<flag value="-Dfdopen=fdopen"/>
</compiler>
')
class Host {
	static function main() {
		cpp.cppia.Host.main();
	}
}
