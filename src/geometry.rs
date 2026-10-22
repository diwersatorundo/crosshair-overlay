use crate::config::{Config, Rgba};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Fill {
        polygons: Vec<Vec<Point>>,
        color: Rgba,
    },
    Ring {
        center: Point,
        radius: f32,
        width: f32,
        color: Rgba,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub shapes: Vec<Shape>,
    pub extent: f32,
}

impl Scene {
    fn push_fill(&mut self, polygons: Vec<Vec<Point>>, color: Rgba) {
        if polygons.is_empty() {
            return;
        }
        for polygon in &polygons {
            for p in polygon {
                self.extend(*p);
            }
        }
        self.shapes.push(Shape::Fill { polygons, color });
    }

    fn push_ring(&mut self, radius: f32, width: f32, color: Rgba) {
        self.extent = self.extent.max(radius + width * 0.5 + 1.0);
        self.shapes.push(Shape::Ring {
            center: Point::new(0.0, 0.0),
            radius,
            width,
            color,
        });
    }

    fn extend(&mut self, p: Point) {
        self.extent = self.extent.max(p.x.abs()).max(p.y.abs());
    }
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Point> {
    vec![
        Point::new(x0, y0),
        Point::new(x1, y0),
        Point::new(x1, y1),
        Point::new(x0, y1),
    ]
}

fn expand_rect(x0: f32, y0: f32, x1: f32, y1: f32, by: f32) -> Vec<Point> {
    rect(x0 - by, y0 - by, x1 + by, y1 + by)
}

fn diagonal_rect(dir: Point, perp: Point, near: f32, far: f32, half_width: f32) -> Vec<Point> {
    let corner = |along: f32, side: f32| {
        Point::new(dir.x * along + perp.x * side, dir.y * along + perp.y * side)
    };
    vec![
        corner(near, -half_width),
        corner(far, -half_width),
        corner(far, half_width),
        corner(near, half_width),
    ]
}

fn scaled(cfg: &Config, screen_height: f32, zoom: f32) -> f32 {
    if !screen_height.is_finite() || screen_height <= 0.0 {
        return 1.0;
    }
    let reference = cfg.screen_height.max(1) as f32;
    let base = if cfg.scale_with_screen {
        screen_height / reference
    } else {
        1.0
    };
    let zoom = if zoom.is_finite() && zoom > 0.0 {
        zoom
    } else {
        1.0
    };
    (base * zoom).clamp(0.02, 128.0)
}

pub fn build_scene(cfg: &Config, screen_height: f32, zoom: f32) -> Scene {
    let scale = scaled(cfg, screen_height, zoom);
    let mut scene = Scene::default();

    let outline_width = if cfg.outline == 0 {
        0.0
    } else {
        (scale.max(1.0)).round()
    };
    let thickness = ((cfg.thickness as f32 * scale).round()).max(1.0);
    let gap = (cfg.gap as f32 * scale).round();
    let length = (cfg.length as f32 * scale).round();
    let half = thickness * 0.5;

    let dot_enabled = cfg.center_dot || cfg.style == 6;
    let show_arms = cfg.style != 6 && length > 0.0;

    match cfg.style {
        1 | 3 => {
            let radius = (gap.max(0.0) + length).max(1.0);
            if outline_width > 0.0 {
                scene.push_ring(radius, thickness + outline_width * 2.0, cfg.outline_color);
            }
            scene.push_ring(radius, thickness, cfg.color);
        }
        7 | 9 => {
            if show_arms {
                let directions = [
                    (
                        Point::new(0.707_106_8, 0.707_106_8),
                        Point::new(-0.707_106_8, 0.707_106_8),
                    ),
                    (
                        Point::new(0.707_106_8, -0.707_106_8),
                        Point::new(0.707_106_8, 0.707_106_8),
                    ),
                    (
                        Point::new(-0.707_106_8, -0.707_106_8),
                        Point::new(0.707_106_8, -0.707_106_8),
                    ),
                    (
                        Point::new(-0.707_106_8, 0.707_106_8),
                        Point::new(-0.707_106_8, -0.707_106_8),
                    ),
                ];
                let near = gap.max(0.0);
                let far = near + length;
                let mut fills = Vec::with_capacity(4);
                let mut outlines = Vec::with_capacity(4);
                for (dir, perp) in directions {
                    fills.push(diagonal_rect(dir, perp, near, far, half));
                    if outline_width > 0.0 {
                        let (near_out, far_out) = if cfg.outline == 2 {
                            (near + (far - near) * 0.5, far)
                        } else {
                            (near - outline_width, far)
                        };
                        outlines.push(diagonal_rect(
                            dir,
                            perp,
                            near_out,
                            far_out + outline_width,
                            half + outline_width,
                        ));
                    }
                }
                scene.push_fill(outlines, cfg.outline_color);
                scene.push_fill(fills, cfg.color);
            }
        }
        8 => {
            if show_arms {
                let half_len = (gap.max(0.0) + length).max(1.0);
                let t = thickness.min(half_len * 2.0);
                let inner = (half_len - t).max(0.0);
                let panels = vec![
                    rect(-half_len, -half_len, half_len, -inner),
                    rect(-half_len, inner, half_len, half_len),
                    rect(-half_len, -inner, -inner, inner),
                    rect(inner, -inner, half_len, inner),
                ];
                if outline_width > 0.0 {
                    let outlines: Vec<Vec<Point>> = panels
                        .iter()
                        .map(|poly| {
                            let xs: Vec<f32> = poly.iter().map(|p| p.x).collect();
                            let ys: Vec<f32> = poly.iter().map(|p| p.y).collect();
                            let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
                            let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                            let min_y = ys.iter().copied().fold(f32::INFINITY, f32::min);
                            let max_y = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                            expand_rect(min_x, min_y, max_x, max_y, outline_width)
                        })
                        .collect();
                    scene.push_fill(outlines, cfg.outline_color);
                }
                scene.push_fill(panels, cfg.color);
            }
        }
        _ => {
            if show_arms {
                let near = gap;
                let far = gap + length;
                if far > near {
                    let mut fills = Vec::with_capacity(4);
                    let mut outlines = Vec::with_capacity(4);

                    let mut arm = |vertical: bool, negative: bool| {
                        let (x0, y0, x1, y1) = match (vertical, negative) {
                            (false, false) => (near, -half, far, half),
                            (false, true) => (-far, -half, -near, half),
                            (true, false) => (-half, near, half, far),
                            (true, true) => (-half, -far, half, -near),
                        };
                        fills.push(rect(x0, y0, x1, y1));
                        if outline_width > 0.0 {
                            let (ox0, oy0, ox1, oy1) = if cfg.outline == 2 {
                                match (vertical, negative) {
                                    (false, false) => {
                                        let mid = x0 + (x1 - x0) * 0.5;
                                        (mid, y0, x1, y1)
                                    }
                                    (false, true) => {
                                        let mid = x0 + (x1 - x0) * 0.5;
                                        (x0, y0, mid, y1)
                                    }
                                    (true, false) => {
                                        let mid = y0 + (y1 - y0) * 0.5;
                                        (x0, mid, x1, y1)
                                    }
                                    (true, true) => {
                                        let mid = y0 + (y1 - y0) * 0.5;
                                        (x0, y0, x1, mid)
                                    }
                                }
                            } else {
                                (x0, y0, x1, y1)
                            };
                            outlines.push(expand_rect(ox0, oy0, ox1, oy1, outline_width));
                        }
                    };

                    arm(false, false);
                    arm(false, true);
                    arm(true, false);
                    if !cfg.t_style {
                        arm(true, true);
                    }

                    scene.push_fill(outlines, cfg.outline_color);
                    scene.push_fill(fills, cfg.color);
                }
            }
        }
    }

    if dot_enabled {
        let d = thickness.max(1.0);
        let half_dot = d * 0.5;
        if outline_width > 0.0 {
            scene.push_fill(
                vec![expand_rect(
                    -half_dot,
                    -half_dot,
                    half_dot,
                    half_dot,
                    outline_width,
                )],
                cfg.outline_color,
            );
        }
        scene.push_fill(
            vec![rect(-half_dot, -half_dot, half_dot, half_dot)],
            cfg.color,
        );
    }

    if scene.extent.is_finite() {
        scene.extent = scene.extent.max(1.0);
    } else {
        scene.extent = 1.0;
    }
    scene
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill_count(scene: &Scene) -> usize {
        scene
            .shapes
            .iter()
            .filter(|s| matches!(s, Shape::Fill { .. }))
            .count()
    }

    #[test]
    fn cross_has_four_arms_and_outline() {
        let cfg = Config::default();
        let scene = build_scene(&cfg, 1080.0, 1.0);
        assert_eq!(fill_count(&scene), 2);
        let Shape::Fill { polygons, .. } = &scene.shapes[1] else {
            panic!("expected fill");
        };
        assert_eq!(polygons.len(), 4);
        assert!(scene.extent >= 5.0);
    }

    #[test]
    fn t_style_removes_top_arm() {
        let cfg = Config {
            t_style: true,
            ..Config::default()
        };
        let scene = build_scene(&cfg, 1080.0, 1.0);
        let Shape::Fill { polygons, .. } = &scene.shapes[1] else {
            panic!("expected fill");
        };
        assert_eq!(polygons.len(), 3);
    }

    #[test]
    fn dot_only_has_no_arms() {
        let cfg = Config {
            style: 6,
            ..Config::default()
        };
        let scene = build_scene(&cfg, 1080.0, 1.0);
        assert_eq!(fill_count(&scene), 2);
        let Shape::Fill { polygons, .. } = &scene.shapes[1] else {
            panic!("expected fill");
        };
        assert_eq!(polygons.len(), 1);
    }

    #[test]
    fn circle_uses_ring() {
        let cfg = Config {
            style: 3,
            ..Config::default()
        };
        let scene = build_scene(&cfg, 1080.0, 1.0);
        assert!(scene.shapes.iter().any(|s| matches!(s, Shape::Ring { .. })));
    }

    #[test]
    fn scaling_doubles_geometry() {
        let cfg = Config::default();
        let a = build_scene(&cfg, 1080.0, 1.0);
        let b = build_scene(&cfg, 2160.0, 1.0);
        assert!(b.extent > a.extent * 1.9);
        let c = build_scene(&cfg, 1080.0, 2.0);
        assert!((c.extent - b.extent).abs() < 0.001);
    }

    #[test]
    fn zero_length_still_shows_dot() {
        let cfg = Config {
            length: 0,
            center_dot: true,
            ..Config::default()
        };
        let scene = build_scene(&cfg, 1080.0, 1.0);
        assert_eq!(fill_count(&scene), 2);
        assert!(!scene.shapes.is_empty());
    }

    #[test]
    fn invalid_zoom_is_safe() {
        let cfg = Config::default();
        let scene = build_scene(&cfg, 1080.0, f32::NAN);
        assert!(scene.extent.is_finite());
    }
}
