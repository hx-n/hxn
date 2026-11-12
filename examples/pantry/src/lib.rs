//! Plain data: values that cross by copy, where an interface is held by
//! reference.

/// What a quantity is counted in. Haxe sees the same names, as numbers.
#[hxn::data]
#[derive(Clone, Copy, PartialEq)]
pub enum Unit {
    Each,
    Kilo,
    Litre,
}

/// Something on a shelf. Haxe sees a class with these fields.
#[hxn::data]
#[derive(Clone)]
pub struct Item {
    pub name: String,
    pub quantity: i32,
    pub unit: Unit,
    /// A field that may be missing can be left out in Haxe.
    pub note: Option<String>,
}

/// The shelves: a value Haxe holds, which data is passed to and from.
#[hxn::interface]
#[derive(Default)]
pub struct Pantry {
    items: Vec<Item>,
}

#[hxn::methods]
impl Pantry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Puts an item on the shelf and returns how many there are. The item
    /// is the library's own copy from here on.
    pub fn stock(&mut self, item: Item) -> Result<i32, String> {
        if item.quantity <= 0 {
            return Err(format!("{} has nothing to stock", item.name));
        }
        self.items.push(item);
        Ok(self.items.len() as i32)
    }

    /// The item of that name, if there is one.
    pub fn find(&self, name: &str) -> Option<Item> {
        self.items.iter().find(|item| item.name == name).cloned()
    }

    /// Everything counted in `unit` of which there is less than `below`.
    pub fn low(&self, unit: Unit, below: i32) -> Vec<Item> {
        self.items
            .iter()
            .filter(|item| item.unit == unit && item.quantity < below)
            .cloned()
            .collect()
    }
}
