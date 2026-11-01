//! Coordinate types for the logical, physical and buffer spaces.

use std::marker::PhantomData;
use std::ops::{Add, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Logical;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Physical;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Buffer;

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

impl Point<f64, Logical> {
    pub fn to_physical(self, scale: f64) -> Point<i32, Physical> {
        Point::new(scale_floor(self.x, scale), scale_floor(self.y, scale))
    }

    pub fn to_buffer(self, scale: f64) -> Point<i32, Buffer> {
        Point::new(scale_floor(self.x, scale), scale_floor(self.y, scale))
    }
}

impl Point<i32, Physical> {
    pub fn to_logical(self, scale: f64) -> Point<f64, Logical> {
        Point::new(scale_down(self.x, scale), scale_down(self.y, scale))
    }
}

impl Point<i32, Buffer> {
    pub fn to_logical(self, scale: f64) -> Point<f64, Logical> {
        Point::new(scale_down(self.x, scale), scale_down(self.y, scale))
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

impl Size<f64, Logical> {
    pub fn to_physical(self, scale: f64) -> Size<i32, Physical> {
        Size::new(
            scale_ceil(self.width, scale),
            scale_ceil(self.height, scale),
        )
    }

    pub fn to_buffer(self, scale: f64) -> Size<i32, Buffer> {
        Size::new(
            scale_ceil(self.width, scale),
            scale_ceil(self.height, scale),
        )
    }
}

impl Size<i32, Physical> {
    pub fn to_logical(self, scale: f64) -> Size<f64, Logical> {
        Size::new(
            scale_down(self.width, scale),
            scale_down(self.height, scale),
        )
    }
}

impl Size<i32, Buffer> {
    pub fn to_logical(self, scale: f64) -> Size<f64, Logical> {
        Size::new(
            scale_down(self.width, scale),
            scale_down(self.height, scale),
        )
    }
}

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

impl Rect<f64, Logical> {
    pub fn to_physical(self, scale: f64) -> Rect<i32, Physical> {
        Rect::new(
            self.location.to_physical(scale),
            self.size.to_physical(scale),
        )
    }

    pub fn to_buffer(self, scale: f64) -> Rect<i32, Buffer> {
        Rect::new(self.location.to_buffer(scale), self.size.to_buffer(scale))
    }
}

impl Rect<i32, Physical> {
    pub fn to_logical(self, scale: f64) -> Rect<f64, Logical> {
        Rect::new(self.location.to_logical(scale), self.size.to_logical(scale))
    }
}

impl Rect<i32, Buffer> {
    pub fn to_logical(self, scale: f64) -> Rect<f64, Logical> {
        Rect::new(self.location.to_logical(scale), self.size.to_logical(scale))
    }
}

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

fn scale_floor(value: f64, scale: f64) -> i32 {
    debug_assert!(scale > 0.0, "coordinate scale must be greater than zero");
    (value * scale).floor() as i32
}

fn scale_ceil(value: f64, scale: f64) -> i32 {
    debug_assert!(scale > 0.0, "coordinate scale must be greater than zero");
    (value * scale).ceil() as i32
}

fn scale_down(value: i32, scale: f64) -> f64 {
    debug_assert!(scale > 0.0, "coordinate scale must be greater than zero");
    f64::from(value) / scale
}

#[cfg(test)]
mod tests {
    use super::{Buffer, Logical, Physical, Point, Rect, Size};

    #[test]
    fn logical_point_scales_up_to_physical() {
        let logical = Point::<f64, Logical>::new(3.0, 4.0);
        assert_eq!(logical.to_physical(2.0), Point::<i32, Physical>::new(6, 8));
    }

    #[test]
    fn a_fractional_logical_point_is_floored() {
        let logical = Point::<f64, Logical>::new(1.25, 1.75);
        assert_eq!(logical.to_physical(2.0), Point::<i32, Physical>::new(2, 3));
    }

    #[test]
    fn a_fractional_logical_size_is_ceiled() {
        let logical = Size::<f64, Logical>::new(1.25, 1.75);
        assert_eq!(logical.to_physical(2.0), Size::<i32, Physical>::new(3, 4));
    }

    #[test]
    fn adjacent_fractional_rectangles_share_their_edge() {
        let left = Rect::<f64, Logical>::new(Point::new(0.0, 0.0), Size::new(1.25, 1.0));
        let right = Rect::<f64, Logical>::new(Point::new(1.25, 0.0), Size::new(1.25, 1.0));
        let left = left.to_physical(1.0);
        let right = right.to_physical(1.0);
        assert_eq!(
            left,
            Rect::<i32, Physical>::new(Point::new(0, 0), Size::new(2, 1))
        );
        assert_eq!(
            right,
            Rect::<i32, Physical>::new(Point::new(1, 0), Size::new(2, 1))
        );
        assert!(
            left.right() > right.x(),
            "the shared edge must not leave a gap"
        );
    }

    #[test]
    fn physical_point_keeps_fraction_when_returning_to_logical() {
        let physical = Point::<i32, Physical>::new(5, 8);
        assert_eq!(
            physical.to_logical(2.0),
            Point::<f64, Logical>::new(2.5, 4.0)
        );
    }

    #[test]
    fn buffer_point_scales_down_to_logical() {
        let buffer = Point::<i32, Buffer>::new(6, 8);
        assert_eq!(buffer.to_logical(2.0), Point::<f64, Logical>::new(3.0, 4.0));
    }

    #[test]
    fn logical_point_scales_up_to_buffer() {
        let logical = Point::<f64, Logical>::new(3.0, 4.0);
        assert_eq!(logical.to_buffer(2.0), Point::<i32, Buffer>::new(6, 8));
    }

    #[test]
    fn scale_of_one_is_the_identity() {
        let logical = Point::<f64, Logical>::new(-5.0, 7.0);
        assert_eq!(logical.to_physical(1.0), Point::<i32, Physical>::new(-5, 7));
    }

    #[test]
    fn logical_size_scales_on_both_axes() {
        let logical = Size::<f64, Logical>::new(1280.0, 720.0);
        assert_eq!(
            logical.to_physical(2.0),
            Size::<i32, Physical>::new(2560, 1440)
        );
    }

    #[test]
    fn logical_size_scales_up_to_buffer() {
        let logical = Size::<f64, Logical>::new(1280.0, 720.0);
        assert_eq!(logical.to_buffer(2.0), Size::<i32, Buffer>::new(2560, 1440));
    }

    #[test]
    fn physical_size_scales_down_to_logical() {
        let physical = Size::<i32, Physical>::new(2560, 1440);
        assert_eq!(
            physical.to_logical(2.0),
            Size::<f64, Logical>::new(1280.0, 720.0)
        );
    }

    #[test]
    fn buffer_size_scales_down_to_logical() {
        let buffer = Size::<i32, Buffer>::new(2560, 1440);
        assert_eq!(
            buffer.to_logical(2.0),
            Size::<f64, Logical>::new(1280.0, 720.0)
        );
    }

    #[test]
    fn logical_rect_scales_location_and_extents() {
        let rect = Rect::<f64, Logical>::new(Point::new(1.0, 2.0), Size::new(10.0, 20.0));
        assert_eq!(
            rect.to_physical(2.0),
            Rect::<i32, Physical>::new(Point::new(2, 4), Size::new(20, 40))
        );
    }

    #[test]
    fn logical_rect_scales_into_buffer_space() {
        let rect = Rect::<f64, Logical>::new(Point::new(1.0, 2.0), Size::new(10.0, 20.0));
        assert_eq!(
            rect.to_buffer(2.0),
            Rect::<i32, Buffer>::new(Point::new(2, 4), Size::new(20, 40))
        );
    }

    #[test]
    fn physical_rect_scales_back_to_logical() {
        let rect = Rect::<i32, Physical>::new(Point::new(2, 4), Size::new(20, 40));
        assert_eq!(
            rect.to_logical(2.0),
            Rect::<f64, Logical>::new(Point::new(1.0, 2.0), Size::new(10.0, 20.0))
        );
    }

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
            Size::<i32, Physical>::default(),
            Size::<i32, Physical>::new(0, 0)
        );
        assert_eq!(Rect::<i32, Buffer>::default().width(), 0);
    }
}
