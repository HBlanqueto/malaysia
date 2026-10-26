//! Coordinate types for the three spaces Yuki works in.
//!
//! These are the scene graph's own types: the compositor uses `smithay::utils`
//! directly and converts into these at the boundary where it feeds the scene,
//! so this crate never depends on Smithay. See ADR 0001.
//!
//! A compositor constantly moves values between three different notions of
//! "position": desktop coordinates that ignore output scale ([`Logical`]),
//! real pixels on a given output ([`Physical`]), and pixels inside a client's
//! buffer ([`Buffer`]). Mixing them is a classic source of subtle bugs, so
//! each is its own type-level space: [`Point`], [`Size`] and [`Rect`] carry a
//! [`PhantomData`] space parameter, and arithmetic is only implemented within
//! a single space.
//!
//! Logical coordinates are `f64`. Yuki is built around spring-driven
//! animation, and a spring interpolating over integers visibly steps right at
//! the end of a motion, where per-frame movement is sub-pixel — exactly where
//! smoothness matters most. Fractional output scales such as 1.25 also place
//! logical positions between physical pixels. Logical values therefore stay
//! fractional, and only the last step before rasterisation rounds them into
//! integer [`Physical`] or [`Buffer`] coordinates.
//!
//! That last step is asymmetric (ADR 0004): a **position is floored** and a
//! **size is ceiled**. Two adjacent rectangles that share a fractional logical
//! edge then share the physical pixel on that edge instead of leaving a
//! one-pixel seam, which is what damage and scissor regions need.
//!
//! Conversions between spaces are never implicit. Each one is a named method
//! that takes the scale factor explicitly, so a call site always shows which
//! space it leaves and which it enters.

use std::marker::PhantomData;
use std::ops::{Add, Sub};

/// Type-level marker for the logical coordinate space.
///
/// Logical coordinates describe the desktop independently of any output's
/// scale factor, and are [`f64`] so that animation and fractional scales are
/// not quantised. A window keeps the same logical geometry when it moves
/// between a 1x and a 2x output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Logical;

/// Type-level marker for the physical coordinate space.
///
/// Physical coordinates are real pixels on one output, after its scale factor
/// has been applied, and are `i32`. Two outputs with different scales have
/// different physical coordinate systems even though they share the logical
/// desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Physical;

/// Type-level marker for the buffer coordinate space.
///
/// Buffer coordinates are integer pixels inside a client's own buffer, before
/// the compositor maps them onto an output. They are the space a client draws
/// in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Buffer;

/// A point in the coordinate space `Space`, with numeric type `N`.
///
/// The space is part of the type, so points from different spaces cannot be
/// combined. This does not compile:
///
/// ```compile_fail
/// use malaysia_scene::geometry::{Logical, Physical, Point};
///
/// let logical = Point::<f64, Logical>::new(1.0, 1.0);
/// let physical = Point::<i32, Physical>::new(2, 2);
/// let _ = logical + physical;
/// ```
///
/// Convert between spaces explicitly with the `to_*` methods, for example
/// [`Point::<f64, Logical>::to_physical`].
#[derive(Debug, PartialEq, Eq, Default)]
pub struct Point<N, Space> {
    /// Horizontal position.
    pub x: N,
    /// Vertical position.
    pub y: N,
    space: PhantomData<Space>,
}

impl<N, Space> Point<N, Space> {
    /// Creates a point at `x`, `y` in the space `Space`.
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
    /// Converts this logical point to the physical space of an output.
    ///
    /// `scale` is the output's scale factor (for example `2.0` on a HiDPI
    /// output) and must be greater than zero. The result is floored to the
    /// physical pixel (ADR 0004); this is the last point at which the
    /// fractional logical position may be discarded.
    pub fn to_physical(self, scale: f64) -> Point<i32, Physical> {
        Point::new(scale_floor(self.x, scale), scale_floor(self.y, scale))
    }

    /// Converts this logical point to the buffer space of a client.
    ///
    /// `scale` is the target scale factor and must be greater than zero. The
    /// result is floored to the buffer pixel (ADR 0004).
    pub fn to_buffer(self, scale: f64) -> Point<i32, Buffer> {
        Point::new(scale_floor(self.x, scale), scale_floor(self.y, scale))
    }
}

impl Point<i32, Physical> {
    /// Converts this physical point back to logical space.
    ///
    /// `scale` is the scale factor that produced the physical point and must
    /// be greater than zero. The result stays fractional: physical pixels do
    /// not always land on a whole logical position, and truncating here would
    /// reintroduce the stepping that `f64` logical coordinates exist to avoid.
    pub fn to_logical(self, scale: f64) -> Point<f64, Logical> {
        Point::new(scale_down(self.x, scale), scale_down(self.y, scale))
    }
}

impl Point<i32, Buffer> {
    /// Converts this buffer point back to logical space.
    ///
    /// `scale` is the buffer's scale factor and must be greater than zero. The
    /// result stays fractional.
    pub fn to_logical(self, scale: f64) -> Point<f64, Logical> {
        Point::new(scale_down(self.x, scale), scale_down(self.y, scale))
    }
}

/// A width and height in the coordinate space `Space`, with numeric type `N`.
///
/// As with [`Point`], sizes from different spaces cannot be combined.
#[derive(Debug, PartialEq, Eq, Default)]
pub struct Size<N, Space> {
    /// Horizontal extent.
    pub width: N,
    /// Vertical extent.
    pub height: N,
    space: PhantomData<Space>,
}

impl<N, Space> Size<N, Space> {
    /// Creates a size with the given `width` and `height` in the space `Space`.
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
    /// Converts this logical size to the physical space of an output.
    ///
    /// `scale` must be greater than zero. Each extent is ceiled to the
    /// physical pixel, so a rectangle always covers the pixels it overlaps
    /// (ADR 0004).
    pub fn to_physical(self, scale: f64) -> Size<i32, Physical> {
        Size::new(
            scale_ceil(self.width, scale),
            scale_ceil(self.height, scale),
        )
    }

    /// Converts this logical size to the buffer space of a client.
    ///
    /// `scale` must be greater than zero. Each extent is ceiled to the buffer
    /// pixel (ADR 0004).
    pub fn to_buffer(self, scale: f64) -> Size<i32, Buffer> {
        Size::new(
            scale_ceil(self.width, scale),
            scale_ceil(self.height, scale),
        )
    }
}

impl Size<i32, Physical> {
    /// Converts this physical size back to logical space.
    ///
    /// `scale` must be greater than zero. The result stays fractional.
    pub fn to_logical(self, scale: f64) -> Size<f64, Logical> {
        Size::new(
            scale_down(self.width, scale),
            scale_down(self.height, scale),
        )
    }
}

impl Size<i32, Buffer> {
    /// Converts this buffer size back to logical space.
    ///
    /// `scale` must be greater than zero. The result stays fractional.
    pub fn to_logical(self, scale: f64) -> Size<f64, Logical> {
        Size::new(
            scale_down(self.width, scale),
            scale_down(self.height, scale),
        )
    }
}

/// A rectangle in the coordinate space `Space`, with numeric type `N`.
///
/// The rectangle is stored as a location and a size, both in the same space,
/// so its corners can be converted without repeating the scaling rules.
#[derive(Debug, PartialEq, Eq, Default)]
pub struct Rect<N, Space> {
    location: Point<N, Space>,
    size: Size<N, Space>,
}

impl<N: Copy, Space> Rect<N, Space> {
    /// Creates a rectangle from a corner and an extent, both in `Space`.
    pub fn new(location: Point<N, Space>, size: Size<N, Space>) -> Self {
        Self { location, size }
    }

    /// Returns the rectangle's top-left corner.
    pub fn location(&self) -> Point<N, Space> {
        self.location
    }

    /// Returns the rectangle's extents.
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
    /// Converts this logical rectangle to the physical space of an output.
    ///
    /// `scale` must be greater than zero. The corner is floored and the
    /// extents are ceiled (ADR 0004).
    pub fn to_physical(self, scale: f64) -> Rect<i32, Physical> {
        Rect::new(
            self.location.to_physical(scale),
            self.size.to_physical(scale),
        )
    }

    /// Converts this logical rectangle to the buffer space of a client.
    ///
    /// `scale` must be greater than zero. The corner is floored and the
    /// extents are ceiled (ADR 0004).
    pub fn to_buffer(self, scale: f64) -> Rect<i32, Buffer> {
        Rect::new(self.location.to_buffer(scale), self.size.to_buffer(scale))
    }
}

impl Rect<i32, Physical> {
    /// Converts this physical rectangle back to logical space.
    ///
    /// `scale` must be greater than zero.
    pub fn to_logical(self, scale: f64) -> Rect<f64, Logical> {
        Rect::new(self.location.to_logical(scale), self.size.to_logical(scale))
    }
}

impl Rect<i32, Buffer> {
    /// Converts this buffer rectangle back to logical space.
    ///
    /// `scale` must be greater than zero.
    pub fn to_logical(self, scale: f64) -> Rect<f64, Logical> {
        Rect::new(self.location.to_logical(scale), self.size.to_logical(scale))
    }
}

impl<N: Copy, Space> Rect<N, Space> {
    /// Returns the x coordinate of the rectangle's top-left corner.
    pub fn x(&self) -> N {
        self.location.x
    }

    /// Returns the y coordinate of the rectangle's top-left corner.
    pub fn y(&self) -> N {
        self.location.y
    }

    /// Returns the rectangle's width.
    pub fn width(&self) -> N {
        self.size.width
    }

    /// Returns the rectangle's height.
    pub fn height(&self) -> N {
        self.size.height
    }
}

impl<N, Space> Rect<N, Space>
where
    N: PartialOrd + Add<Output = N> + Copy,
{
    /// Whether `point` lies inside the rectangle.
    ///
    /// The right and bottom edges are exclusive, so two adjacent rectangles
    /// never both claim a point on the boundary between them.
    pub fn contains(&self, point: Point<N, Space>) -> bool {
        point.x >= self.location.x
            && point.x < self.location.x + self.size.width
            && point.y >= self.location.y
            && point.y < self.location.y + self.size.height
    }

    /// The x coordinate just past the rectangle's right edge.
    pub fn right(&self) -> N {
        self.location.x + self.size.width
    }

    /// The y coordinate just past the rectangle's bottom edge.
    pub fn bottom(&self) -> N {
        self.location.y + self.size.height
    }
}

/// Scales a logical position up and floors it to the integer pixel (ADR 0004).
///
/// Called only when entering an integer space, so the fractional value is
/// discarded here and nowhere earlier. `scale` must be greater than zero.
fn scale_floor(value: f64, scale: f64) -> i32 {
    debug_assert!(scale > 0.0, "coordinate scale must be greater than zero");
    (value * scale).floor() as i32
}

/// Scales a logical extent up and ceils it to the integer pixel (ADR 0004).
///
/// `scale` must be greater than zero.
fn scale_ceil(value: f64, scale: f64) -> i32 {
    debug_assert!(scale > 0.0, "coordinate scale must be greater than zero");
    (value * scale).ceil() as i32
}

/// Scales an integer pixel coordinate back down to a fractional logical value.
///
/// The result is not rounded: physical pixels do not always land on a whole
/// logical position. `scale` must be greater than zero.
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
        // Two logical rectangles that share the 1.25 edge, at scale 1: the
        // first covers 0..2 and the second 1..3, so the edge pixel is drawn
        // by both and no seam is left (ADR 0004).
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
