/**
	The hxcpp host program that runs the fixture's CPPIA script. It contains
	the library's generated classes and hxn's classes, and takes the script's
	path as its argument.
**/
// hxcpp's bundled zlib, which a CPPIA host links, treats `fdopen` as missing
// on macOS unless it is defined as a macro.
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
