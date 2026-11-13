//! Plain data: values passed across the boundary by copy, unlike an
//! interface, which is held by reference. A `#[hxn::data]` error type is
//! passed the same way.

/// The unit a quantity is counted in. A C-like enum: Haxe gets the same
/// names, with integer values.
#[hxn::data]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Unit {
    Each,
    Kilo,
    Litre,
}

/// An item on a shelf. Haxe gets a class with these fields.
#[hxn::data]
#[derive(Clone)]
pub struct Item {
    pub name: String,
    pub quantity: i32,
    pub unit: Unit,
    /// An optional field can be omitted in Haxe.
    pub note: Option<String>,
}

/// The error type of `take`. A tagged enum becomes a Haxe enum with the same
/// constructors. Used as an error type, it is thrown as `PantryException`.
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

/// The shelves: an interface Haxe holds by reference. Its methods accept and
/// return data.
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

    /// Puts an item on the shelf and returns the number of items. The
    /// library stores its own copy of the item.
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

    /// Returns the item with that name, or `None` if there is none.
    pub fn find(&self, name: &str) -> Option<Item> {
        self.items.iter().find(|item| item.name == name).cloned()
    }

    /// Requested names that are not stocked, returned once each in order.
    pub fn missing(
        &self,
        wanted: &std::collections::HashSet<String>,
    ) -> std::collections::BTreeSet<String> {
        wanted
            .iter()
            .filter(|name| !self.items.iter().any(|item| item.name == **name))
            .cloned()
            .collect()
    }

    /// Quantities still needed to reach a target for each unit.
    pub fn shortages(
        &self,
        wanted: &std::collections::BTreeMap<Unit, i32>,
    ) -> std::collections::BTreeMap<Unit, i32> {
        wanted
            .iter()
            .map(|(unit, wanted)| {
                let stocked: i32 = self
                    .items
                    .iter()
                    .filter(|item| item.unit == *unit)
                    .map(|item| item.quantity)
                    .sum();
                (*unit, (wanted - stocked).max(0))
            })
            .collect()
    }

    /// Returns the items counted in `unit` whose quantity is less than
    /// `below`.
    pub fn low(&self, unit: Unit, below: i32) -> Vec<Item> {
        self.items
            .iter()
            .filter(|item| item.unit == unit && item.quantity < below)
            .cloned()
            .collect()
    }
}
