package hxn;

#if hl
/**
	A value a library returned, as HashLink holds it: an abstract the VM
	releases through the library when it is collected.
**/
typedef Handle = hl.Abstract<"hxn_handle">;
#elseif cpp
/**
	A value a library returned, as hxcpp holds it. The library's native class
	gives it a finalizer that releases the value through the library, once
	the call that returned it has returned.
**/
@:noCompletion class Handle {
	@:unreflective public var raw:cpp.RawPointer<cpp.Void>;

	// No argument: hxcpp's reflective constructor and CPPIA's would pass a
	// Dynamic, which some hxcpp releases cannot convert to a pointer.
	public function new() {}
}
#else
/** On a target hxn does not bind, only for typing: in an editor, say. **/
typedef Handle = Dynamic;
#end
