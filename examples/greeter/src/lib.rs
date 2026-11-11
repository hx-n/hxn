//! A library for Haxe, in plain Rust. `hxn` reads the items marked here and
//! writes the binding for each runtime.

/// Greets people, and counts them.
#[hxn::interface]
pub struct Greeter {
    greeting: String,
    greeted: i32,
}

#[hxn::methods]
impl Greeter {
    pub fn new(greeting: &str) -> Self {
        Self {
            greeting: greeting.to_owned(),
            greeted: 0,
        }
    }

    /// A greeting for `name`. Fails for an empty name.
    pub fn greet(&mut self, name: &str) -> Result<String, &'static str> {
        if name.is_empty() {
            return Err("nobody to greet");
        }
        self.greeted += 1;
        Ok(format!("{}, {name}!", self.greeting))
    }

    /// How many have been greeted.
    pub fn greeted(&self) -> i32 {
        self.greeted
    }
}
