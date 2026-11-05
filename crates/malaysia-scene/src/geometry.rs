use std::marker::PhantomData;
use std::ops::{Add, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Logical;

#[derive(Debug, PartialEq, Eq, Default)]
pub struct Point<N, Space> {
    pub x: N,
    pub y: N,
    space: PhantomData<Space>,
}

impl<N, Space> Point<N, Space> {
    pub fn new(x: N, y: N) -> Self {
        Self {
            x,
            y,
            space: PhantomData,
        }
    }
}

impl<N: Copy, Space> Clone for Point<N, Space> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<N: Copy, Space> Copy for Point<N, Space> {}

impl<N: Add<Output = N>, Space> Add for Point<N, Space> {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl<N: Sub<Output = N>, Space> Sub for Point<N, Space> {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

#[derive(Debug, PartialEq, Eq, Default)]
pub struct Size<N, Space> {
    pub width: N,
    pub height: N,
    space: PhantomData<Space>,
}

impl<N, Space> Size<N, Space> {
    pub fn new(width: N, height: N) -> Self {
        Self {
            width,
            height,
            space: PhantomData,
        }
    }
}

impl<N: Copy, Space> Clone for Size<N, Space> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<N: Copy, Space> Copy for Size<N, Space> {}

#[derive(Debug, PartialEq, Eq, Default)]
pub struct Rect<N, Space> {
    location: Point<N, Space>,
    size: Size<N, Space>,
}

impl<N: Copy, Space> Rect<N, Space> {
    pub fn new(location: Point<N, Space>, size: Size<N, Space>) -> Self {
        Self { location, size }
    }

    pub fn location(&self) -> Point<N, Space> {
        self.location
    }

    pub fn size(&self) -> Size<N, Space> {
        self.size
    }
}

impl<N: Copy, Space> Clone for Rect<N, Space> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<N: Copy, Space> Copy for Rect<N, Space> {}

impl<N: Copy, Space> Rect<N, Space> {
    pub fn x(&self) -> N {
        self.location.x
    }

    pub fn y(&self) -> N {
        self.location.y
    }

    pub fn width(&self) -> N {
        self.size.width
    }

    pub fn height(&self) -> N {
        self.size.height
    }
}

impl<N, Space> Rect<N, Space>
where
    N: PartialOrd + Add<Output = N> + Copy,
{
    pub fn contains(&self, point: Point<N, Space>) -> bool {
        point.x >= self.location.x
            && point.x < self.location.x + self.size.width
            && point.y >= self.location.y
            && point.y < self.location.y + self.size.height
    }

    pub fn right(&self) -> N {
        self.location.x + self.size.width
    }

    pub fn bottom(&self) -> N {
        self.location.y + self.size.height
    }
}

#[cfg(test)]
mod tests {
    use super::{Logical, Point, Rect, Size};

    #[test]
    fn rect_exposes_its_parts() {
        let rect = Rect::<f64, Logical>::new(Point::new(1.0, 2.0), Size::new(10.0, 20.0));
        assert_eq!(rect.x(), 1.0);
        assert_eq!(rect.y(), 2.0);
        assert_eq!(rect.width(), 10.0);
        assert_eq!(rect.height(), 20.0);
        assert_eq!(rect.location(), Point::<f64, Logical>::new(1.0, 2.0));
        assert_eq!(rect.size(), Size::<f64, Logical>::new(10.0, 20.0));
    }

    #[test]
    fn contains_includes_the_top_left_and_excludes_the_bottom_right() {
        let rect = Rect::<f64, Logical>::new(Point::new(1.0, 2.0), Size::new(10.0, 20.0));
        assert!(rect.contains(Point::new(1.0, 2.0)));
        assert!(rect.contains(Point::new(10.9, 21.9)));
        assert!(!rect.contains(Point::new(11.0, 5.0)));
        assert!(!rect.contains(Point::new(5.0, 22.0)));
        assert!(!rect.contains(Point::new(0.9, 5.0)));
    }

    #[test]
    fn right_and_bottom_are_just_past_the_far_edges() {
        let rect = Rect::<f64, Logical>::new(Point::new(1.0, 2.0), Size::new(10.0, 20.0));
        assert_eq!(rect.right(), 11.0);
        assert_eq!(rect.bottom(), 22.0);
    }

    #[test]
    fn points_in_the_same_space_add_and_subtract() {
        let a = Point::<f64, Logical>::new(1.0, 2.0);
        let b = Point::<f64, Logical>::new(10.0, 20.0);
        assert_eq!(a + b, Point::<f64, Logical>::new(11.0, 22.0));
        assert_eq!(b - a, Point::<f64, Logical>::new(9.0, 18.0));
    }

    #[test]
    fn default_values_are_zero() {
        assert_eq!(
            Point::<f64, Logical>::default(),
            Point::<f64, Logical>::new(0.0, 0.0)
        );
        assert_eq!(
            Size::<i32, Logical>::default(),
            Size::<i32, Logical>::new(0, 0)
        );
        assert_eq!(Rect::<i32, Logical>::default().width(), 0);
    }
}
