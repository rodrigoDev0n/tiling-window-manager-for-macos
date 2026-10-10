//! Platform-independent geometry primitives.
//!
//! Coordinates follow the macOS Accessibility convention: the origin is the
//! top-left corner of the primary display and the Y axis grows downward.

/// A point in screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// A point is valid when both components are finite numbers.
    pub fn is_valid(&self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// A width/height pair measured in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    /// A size is valid when it is finite and strictly positive.
    pub fn is_valid(&self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width > 0.0 && self.height > 0.0
    }
}

/// An axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    /// A rectangle is valid when its origin and size are both valid.
    pub fn is_valid(&self) -> bool {
        self.origin.is_valid() && self.size.is_valid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_validity() {
        assert!(Point::new(10.0, 20.0).is_valid());
        assert!(!Point::new(f64::NAN, 0.0).is_valid());
        assert!(!Point::new(0.0, f64::INFINITY).is_valid());
    }

    #[test]
    fn size_requires_positive_finite_dimensions() {
        assert!(Size::new(100.0, 50.0).is_valid());
        assert!(!Size::new(0.0, 50.0).is_valid());
        assert!(!Size::new(-1.0, 50.0).is_valid());
        assert!(!Size::new(f64::NAN, 50.0).is_valid());
    }

    #[test]
    fn rect_validity_combines_origin_and_size() {
        assert!(Rect::new(0.0, 25.0, 1440.0, 800.0).is_valid());
        assert!(!Rect::new(0.0, 25.0, 0.0, 800.0).is_valid());
        assert!(!Rect::new(f64::NAN, 25.0, 1440.0, 800.0).is_valid());
    }
}
