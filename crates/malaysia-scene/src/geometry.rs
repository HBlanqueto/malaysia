use std::marker::PhantomData;
use std::ops::Add;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Logical;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect<N, Space> {
    location: Point<N, Space>,
    size: Size<N, Space>,
}

impl<N: Copy, Space: Copy> Rect<N, Space> {
    pub fn new(location: Point<N, Space>, size: Size<N, Space>) -> Self {
        Self { location, size }
    }

    pub fn size(&self) -> Size<N, Space> {
        self.size
    }

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
    Space: Copy,
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
    fn contains_includes_the_top_left_and_excludes_the_bottom_right() {
        let rect = Rect::<f64, Logical>::new(Point::new(1.0, 2.0), Size::new(10.0, 20.0));
        assert!(rect.contains(Point::new(1.0, 2.0)));
        assert!(rect.contains(Point::new(10.9, 21.9)));
        assert!(!rect.contains(Point::new(11.0, 5.0)));
        assert!(!rect.contains(Point::new(5.0, 22.0)));
        assert!(!rect.contains(Point::new(0.9, 5.0)));
    }
}
