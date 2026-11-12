// Entry point of the iOS test executable: starts the hxcpp library, as an
// app would.
#include <stdio.h>
extern "C" const char *hxRunLibrary();
extern "C" void hxcpp_set_top_of_stack();
int main() {
	hxcpp_set_top_of_stack();
	const char *error = hxRunLibrary();
	if (error) {
		fprintf(stderr, "%s\n", error);
		return 1;
	}
	return 0;
}
