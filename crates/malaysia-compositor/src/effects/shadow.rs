use smithay::backend::renderer::element::{Element, Id, RenderElement};
use smithay::backend::renderer::gles::{
    GlesError, GlesFrame, GlesPixelProgram, GlesRenderer, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::utils::user_data::UserDataMap;
use smithay::utils::{Buffer, Physical, Point, Rectangle, Scale, Size};

use super::{Drawn, Shape};

pub fn compile(renderer: &mut GlesRenderer) -> Result<GlesPixelProgram, GlesError> {
    renderer.compile_custom_pixel_shader(
        concat!(
            include_str!("shadow.frag"),
            include_str!("corner/coverage.glsl")
        ),
        &[
            UniformName::new("origin", UniformType::_2f),
            UniformName::new("bounds", UniformType::_2f),
            UniformName::new("radius", UniformType::_1f),
            UniformName::new("width", UniformType::_1f),
            UniformName::new("sigma", UniformType::_1f),
            UniformName::new("y_offset", UniformType::_1f),
            UniformName::new("spread", UniformType::_1f),
            UniformName::new("curve", UniformType::_4f),
            UniformName::new("rise", UniformType::_1f),
        ],
    )
}

pub struct Shadow {
    id: Id,
    commit: CommitCounter,
    area: Rectangle<i32, Physical>,
    origin: [f32; 2],
    width: f32,
    shape: Shape,
    sigma: f32,
    y_offset: f32,
    spread: f32,
    opacity: f32,
    program: GlesPixelProgram,
}

impl Shadow {
    pub fn new(
        drawn: &Drawn,
        shape: Shape,
        width: i32,
        scale: f64,
        main: bool,
        dark: bool,
        program: GlesPixelProgram,
    ) -> Shadow {
        let shadows = if dark {
            ctdesign::WINDOW_SHADOW_DARK
        } else {
            ctdesign::WINDOW_SHADOW_LIGHT
        };
        let shadow = if main { shadows.main } else { shadows.inactive };
        let scale = scale as f32;
        let sigma = shadow.sigma * scale;
        let y_offset = shadow.y_offset * scale;
        let spread = ctdesign::WINDOW_SHADOW_SPREAD * scale;
        let reach = (3.0 * sigma + spread).ceil() as i32;
        let above = reach - y_offset.floor() as i32;
        let below = reach + y_offset.ceil() as i32;
        let area = Rectangle::new(
            shape.bounds.loc - Point::from((reach, above)),
            shape.bounds.size + Size::from((2 * reach, above + below)),
        );
        Shadow {
            id: drawn.shadow.clone(),
            commit: drawn.commit,
            area,
            origin: [reach as f32, above as f32],
            width: width as f32,
            shape,
            sigma,
            y_offset,
            spread,
            opacity: shadow.opacity,
            program,
        }
    }
}

impl Element for Shadow {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        Rectangle::from_size(Size::from((
            self.area.size.w as f64,
            self.area.size.h as f64,
        )))
    }

    fn geometry(&self, _scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.area
    }
}

impl RenderElement<GlesRenderer> for Shadow {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
        _cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        frame.render_pixel_shader_to(
            &self.program,
            src,
            dst,
            Size::from((self.area.size.w, self.area.size.h)),
            Some(damage),
            self.opacity,
            &[
                Uniform::new("origin", self.origin),
                Uniform::new(
                    "bounds",
                    [
                        self.shape.bounds.size.w as f32,
                        self.shape.bounds.size.h as f32,
                    ],
                ),
                Uniform::new("radius", self.shape.radius),
                Uniform::new("width", self.width),
                Uniform::new("sigma", self.sigma),
                Uniform::new("y_offset", self.y_offset),
                Uniform::new("spread", self.spread),
                Uniform::new("curve", self.shape.curve.bezier),
                Uniform::new("rise", self.shape.curve.rise),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use ctdesign::BoxShadow;

    const CAPTURE: &str = include_str!("shadow-macos.csv");
    const BOUNDS: [f32; 2] = [600.0, 400.0];

    fn erf(x: f32) -> f32 {
        let a = x.abs();
        let v = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
        let v = v * v;
        x.signum() - x.signum() / (v * v)
    }

    fn gaussian(x: f32, sigma: f32) -> f32 {
        (-(x * x) / (2.0 * sigma * sigma)).exp() / ((2.0 * PI).sqrt() * sigma)
    }

    fn blurred_row(x: f32, y: f32, sigma: f32, half_size: [f32; 2], corner: f32) -> f32 {
        let delta = (half_size[1] - corner - y.abs()).min(0.0);
        let curved = half_size[0] - corner + (corner * corner - delta * delta).max(0.0).sqrt();
        let scale = 0.5_f32.sqrt() / sigma;
        0.5 * (erf((x + curved) * scale) - erf((x - curved) * scale))
    }

    fn shadow(window: [f32; 2], shadow: BoxShadow) -> f32 {
        let spread = ctdesign::WINDOW_SHADOW_SPREAD;
        let sigma = shadow.sigma;
        let half_size = [BOUNDS[0] / 2.0 + spread, BOUNDS[1] / 2.0 + spread];
        let point = [
            window[0] - BOUNDS[0] / 2.0,
            window[1] - BOUNDS[1] / 2.0 - shadow.y_offset,
        ];
        let corner = ctdesign::CORNER_WINDOW.fitted + spread;
        let start = (-3.0 * sigma).clamp(point[1] - half_size[1], point[1] + half_size[1]);
        let end = (3.0 * sigma).clamp(point[1] - half_size[1], point[1] + half_size[1]);
        let row_height = (end - start) / 8.0;
        (0..8)
            .map(|row| start + row_height * (row as f32 + 0.5))
            .map(|y| {
                blurred_row(point[0], point[1] - y, sigma, half_size, corner)
                    * gaussian(y, sigma)
                    * row_height
            })
            .sum::<f32>()
            * shadow.opacity
    }

    fn outside(side: &str, distance: f32) -> [f32; 2] {
        let past_the_frame = distance + 0.5;
        match side {
            "top" => [BOUNDS[0] / 2.0, -past_the_frame],
            "bottom" => [BOUNDS[0] / 2.0, BOUNDS[1] + past_the_frame],
            _ => [-past_the_frame, BOUNDS[1] / 2.0],
        }
    }

    #[test]
    fn a_window_shadow_is_the_macos_capture_above_below_and_beside() {
        let mut rows = CAPTURE.lines();
        let distances: Vec<f32> = rows
            .next()
            .unwrap()
            .split(',')
            .skip(3)
            .map(|distance| distance.parse().unwrap())
            .collect();
        let (mut squares, mut worst, mut count) = (0.0_f32, 0.0_f32, 0);
        for row in rows {
            let fields: Vec<&str> = row.split(',').collect();
            let shadows = match fields[0] {
                "light" => ctdesign::WINDOW_SHADOW_LIGHT,
                _ => ctdesign::WINDOW_SHADOW_DARK,
            };
            let box_shadow = match fields[1] {
                "main" => shadows.main,
                _ => shadows.inactive,
            };
            for (distance, expected) in distances.iter().zip(&fields[3..]) {
                let expected: f32 = expected.parse().unwrap();
                let error = shadow(outside(fields[2], *distance), box_shadow) - expected;
                squares += error * error;
                worst = worst.max(error.abs());
                count += 1;
            }
        }
        let rms = (squares / count as f32).sqrt();
        assert!(rms < 0.002, "rms {rms}");
        assert!(worst < 0.005, "max {worst}");
    }
}
