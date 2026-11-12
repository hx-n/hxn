//! Plain data: values that cross by copy, where an interface is held by
//! reference. A `#[hxn::data]` error type is passed the same way.

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

/// Why `take` failed. A tagged enum becomes a Haxe enum with the same
/// constructors; as an error type it is thrown as `PantryException`.
#[hxn::data]
#[derive(Clone, Debug)]
pub enum PantryError {
    /// No item has that name.
    Unknown(String),
    /// Fewer are in stock than requested.
    Short { name: String, left: i32 },
}

impl std::fmt::Display for PantryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(name) => write!(f, "nothing called {name}"),
            Self::Short { name, left } => write!(f, "there are only {left} of {name}"),
        }
    }
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

    /// Removes `amount` of an item and returns the updated item.
    pub fn take(&mut self, name: &str, amount: i32) -> Result<Item, PantryError> {
        let Some(item) = self.items.iter_mut().find(|item| item.name == name) else {
            return Err(PantryError::Unknown(name.to_owned()));
        };
        if item.quantity < amount {
            return Err(PantryError::Short {
                name: name.to_owned(),
                left: item.quantity,
            });
        }
        item.quantity -= amount;
        Ok(item.clone())
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
