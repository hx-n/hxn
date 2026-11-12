
// Written by hand beside the bindings: a function the WebIDL does not
// describe, calling C through the object's pointer.
#[hxn::methods]
impl Tally {
    /// The total, twice over.
    pub fn doubled(&mut self) -> i64 {
        let total = unsafe { bindings::ffi::tally_total(self.raw) };
        total * 2
    }
}
