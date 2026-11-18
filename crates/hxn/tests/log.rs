#[test]
fn core_macros_skip_disabled_arguments_and_targets() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    fn value() -> usize {
        CALLS.fetch_add(1, Ordering::SeqCst)
    }
    fn target() -> &'static str {
        value();
        "test"
    }
    hxn::log::set_level(hxn::log::Level::Off);
    hxn::log::error!(target: target(), "{}", value());
    hxn::log::warn!("{}", value());
    hxn::log::info!("{}", value());
    hxn::log::debug!("{}", value());
    hxn::log::trace!("{}", value());
    assert_eq!(CALLS.load(Ordering::SeqCst), 0);
}
