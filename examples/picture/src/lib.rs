//! A Haxe function called by the library, bytes handed back, and one object
//! passed to another.

/// A greyscale picture.
#[hxn::interface]
pub struct Picture {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}

#[hxn::methods]
impl Picture {
    /// A black picture. Fails unless each side is from 1 to 4096.
    pub fn new(width: i32, height: i32) -> Result<Self, &'static str> {
        if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) {
            return Err("each side of a picture is from 1 to 4096");
        }
        Ok(Self {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
        })
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    /// Sets every pixel to what `shade` returns for its column and row,
    /// from 0 for black to 255 for white. `shade` is a Haxe function: if it
    /// throws, painting stops there and the caller catches what it threw.
    pub fn paint<E>(&mut self, shade: impl Fn(i32, i32) -> Result<i32, E>) -> Result<(), E> {
        for y in 0..self.height {
            for x in 0..self.width {
                let level = shade(x, y)?.clamp(0, 255);
                self.pixels[(y * self.width + x) as usize] = level as u8;
            }
        }
        Ok(())
    }

    /// Keeps the brighter of each pixel and the one at its place in `other`.
    /// Fails if the two differ in size.
    pub fn lighten(&mut self, other: &Picture) -> Result<(), &'static str> {
        if (self.width, self.height) != (other.width, other.height) {
            return Err("the pictures differ in size");
        }
        for (pixel, theirs) in self.pixels.iter_mut().zip(&other.pixels) {
            *pixel = (*pixel).max(*theirs);
        }
        Ok(())
    }

    /// The pixels, row after row from the top.
    pub fn pixels(&self) -> Vec<u8> {
        self.pixels.clone()
    }
}
