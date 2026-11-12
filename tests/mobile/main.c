/* Entry point of the Android test executable: starts the hxcpp library, as
 * an app would through JNI. */
void hxcpp_main(void);

int main(void) {
	hxcpp_main();
	return 0;
}
