use std::f64::consts::FRAC_1_SQRT_2;

use bytemuck::{Pod, Zeroable};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    Rectangle,
    Circle,
    Triangle,
    RightTriangle,
    Diamond,
    Pentagon,
    Hexagon,
    Octagon,
}

const RECTANGLE: [[f64; 2]; 4] = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
const TRIANGLE: [[f64; 2]; 3] = [[0.0, -0.5], [0.5, 0.5], [-0.5, 0.5]];
const RIGHT_TRIANGLE: [[f64; 2]; 3] = [[-0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
const DIAMOND: [[f64; 2]; 4] = [[0.0, -0.5], [0.5, 0.0], [0.0, 0.5], [-0.5, 0.0]];
const PENTAGON: [[f64; 2]; 5] = [
    [0.0, -0.5],
    [0.5, -0.118_033_988_749_894_9],
    [0.309_016_994_374_947_45, 0.5],
    [-0.309_016_994_374_947_45, 0.5],
    [-0.5, -0.118_033_988_749_894_9],
];
const HEXAGON: [[f64; 2]; 6] = [[0.0, -0.5], [0.5, -0.25], [0.5, 0.25], [0.0, 0.5], [-0.5, 0.25], [-0.5, -0.25]];
const OCTAGON_CORNER: f64 = 0.207_106_781_186_547_52;
const OCTAGON: [[f64; 2]; 8] = [
    [OCTAGON_CORNER, -0.5],
    [0.5, -OCTAGON_CORNER],
    [0.5, OCTAGON_CORNER],
    [OCTAGON_CORNER, 0.5],
    [-OCTAGON_CORNER, 0.5],
    [-0.5, OCTAGON_CORNER],
    [-0.5, -OCTAGON_CORNER],
    [-OCTAGON_CORNER, -0.5],
];

impl ShapeKind {
    pub const ALL: [ShapeKind; 8] = [
        ShapeKind::Rectangle,
        ShapeKind::Circle,
        ShapeKind::Triangle,
        ShapeKind::RightTriangle,
        ShapeKind::Diamond,
        ShapeKind::Pentagon,
        ShapeKind::Hexagon,
        ShapeKind::Octagon,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ShapeKind::Rectangle => "Rectangle",
            ShapeKind::Circle => "Circle",
            ShapeKind::Triangle => "Triangle",
            ShapeKind::RightTriangle => "RightTriangle",
            ShapeKind::Diamond => "Diamond",
            ShapeKind::Pentagon => "Pentagon",
            ShapeKind::Hexagon => "Hexagon",
            ShapeKind::Octagon => "Octagon",
        }
    }

    pub fn from_name(name: &str) -> Option<ShapeKind> {
        Self::ALL.into_iter().find(|shape| shape.name() == name)
    }

    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|shape| *shape == self).unwrap_or(0) as u32
    }

    pub fn outline(self) -> Option<&'static [[f64; 2]]> {
        match self {
            ShapeKind::Rectangle => Some(&RECTANGLE),
            ShapeKind::Circle => None,
            ShapeKind::Triangle => Some(&TRIANGLE),
            ShapeKind::RightTriangle => Some(&RIGHT_TRIANGLE),
            ShapeKind::Diamond => Some(&DIAMOND),
            ShapeKind::Pentagon => Some(&PENTAGON),
            ShapeKind::Hexagon => Some(&HEXAGON),
            ShapeKind::Octagon => Some(&OCTAGON),
        }
    }
}

type Point = [f64; 2];

fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}

fn scale(a: Point, factor: f64) -> Point {
    [a[0] * factor, a[1] * factor]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: Point, b: Point) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn length(a: Point) -> f64 {
    a[0].hypot(a[1])
}

fn rotate(a: Point, cos: f64, sin: f64) -> Point {
    [a[0] * cos - a[1] * sin, a[0] * sin + a[1] * cos]
}

fn finite(a: Point) -> bool {
    a[0].is_finite() && a[1].is_finite()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collider {
    pub id: u64,
    pub position: Point,
    pub size: Point,
    pub anchor: Point,
    pub rotation: f64,
    pub shape: ShapeKind,
}

impl Collider {
    fn offset(&self) -> Point {
        [(self.anchor[0] - 0.5) * self.size[0], (self.anchor[1] - 0.5) * self.size[1]]
    }

    pub fn to_local(&self, point: Point) -> Point {
        let (sin, cos) = self.rotation.sin_cos();
        add(rotate(sub(point, self.position), cos, -sin), self.offset())
    }

    pub fn to_world(&self, local: Point) -> Point {
        let (sin, cos) = self.rotation.sin_cos();
        add(self.position, rotate(sub(local, self.offset()), cos, sin))
    }

    fn polygon(&self) -> Option<Vec<Point>> {
        self.shape
            .outline()
            .map(|outline| outline.iter().map(|point| [point[0] * self.size[0], point[1] * self.size[1]]).collect())
    }

    fn radii(&self) -> Point {
        [self.size[0].abs() * 0.5, self.size[1].abs() * 0.5]
    }

    fn usable(&self) -> bool {
        finite(self.position)
            && finite(self.size)
            && finite(self.anchor)
            && self.rotation.is_finite()
            && self.size[0] != 0.0
            && self.size[1] != 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Query {
    Point(Point),
    Area { center: Point, size: Point, rotation: f64 },
    Radius { center: Point, radius: f64 },
    Ray { origin: Point, direction: Point },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub id: u64,
    pub distance: f64,
    pub position: Point,
    pub normal: Point,
}

fn inside_polygon(points: &[Point], point: Point) -> bool {
    let mut positive = false;
    let mut negative = false;
    for (index, start) in points.iter().enumerate() {
        let end = points[(index + 1) % points.len()];
        let side = cross(sub(end, *start), sub(point, *start));
        if side > 0.0 {
            positive = true;
        } else if side < 0.0 {
            negative = true;
        }
    }
    !(positive && negative)
}

fn inside_ellipse(radii: Point, point: Point) -> bool {
    let x = point[0] / radii[0];
    let y = point[1] / radii[1];
    x * x + y * y <= 1.0
}

fn segment_distance(point: Point, start: Point, end: Point) -> f64 {
    let edge = sub(end, start);
    let squared = dot(edge, edge);
    let t = if squared > 0.0 {
        (dot(sub(point, start), edge) / squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    length(sub(point, add(start, scale(edge, t))))
}

fn edge_distance(points: &[Point], point: Point) -> f64 {
    points
        .iter()
        .enumerate()
        .map(|(index, start)| segment_distance(point, *start, points[(index + 1) % points.len()]))
        .fold(f64::INFINITY, f64::min)
}

fn project(points: &[Point], axis: Point) -> (f64, f64) {
    points
        .iter()
        .map(|point| dot(*point, axis))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), value| (low.min(value), high.max(value)))
}

fn polygons_overlap(first: &[Point], second: &[Point]) -> bool {
    for polygon in [first, second] {
        for (index, start) in polygon.iter().enumerate() {
            let edge = sub(polygon[(index + 1) % polygon.len()], *start);
            let axis = [-edge[1], edge[0]];
            if axis == [0.0, 0.0] {
                continue;
            }
            let (first_low, first_high) = project(first, axis);
            let (second_low, second_high) = project(second, axis);
            if first_high < second_low || second_high < first_low {
                return false;
            }
        }
    }
    true
}

fn ellipse_polygon_overlap(radii: Point, polygon: &[Point]) -> bool {
    let unit: Vec<Point> = polygon.iter().map(|point| [point[0] / radii[0], point[1] / radii[1]]).collect();
    inside_polygon(&unit, [0.0, 0.0]) || edge_distance(&unit, [0.0, 0.0]) <= 1.0
}

pub fn closest_on_ellipse(radii: Point, point: Point) -> Point {
    let (a, b) = (radii[0], radii[1]);
    let (px, py) = (point[0].abs(), point[1].abs());
    let (mut tx, mut ty) = (FRAC_1_SQRT_2, FRAC_1_SQRT_2);
    for _ in 0..4 {
        let x = a * tx;
        let y = b * ty;
        let ex = (a * a - b * b) * tx.powi(3) / a;
        let ey = (b * b - a * a) * ty.powi(3) / b;
        let (rx, ry) = (x - ex, y - ey);
        let (qx, qy) = (px - ex, py - ey);
        let r = rx.hypot(ry);
        let q = qx.hypot(qy).max(1e-12);
        tx = ((qx * r / q + ex) / a).clamp(0.0, 1.0);
        ty = ((qy * r / q + ey) / b).clamp(0.0, 1.0);
        let t = tx.hypot(ty).max(1e-12);
        tx /= t;
        ty /= t;
    }
    [(a * tx).copysign(point[0]), (b * ty).copysign(point[1])]
}

fn signed_area(points: &[Point]) -> f64 {
    points
        .iter()
        .enumerate()
        .map(|(index, start)| cross(*start, points[(index + 1) % points.len()]))
        .sum::<f64>()
        * 0.5
}

fn ray_polygon(points: &[Point], origin: Point, direction: Point) -> Option<(f64, Point)> {
    let orientation = signed_area(points).signum();
    let mut enter = f64::NEG_INFINITY;
    let mut exit = f64::INFINITY;
    let mut normal = [0.0, 0.0];
    for (index, start) in points.iter().enumerate() {
        let edge = sub(points[(index + 1) % points.len()], *start);
        let outward = if orientation > 0.0 { [edge[1], -edge[0]] } else { [-edge[1], edge[0]] };
        let size = length(outward);
        if size == 0.0 {
            continue;
        }
        let outward = scale(outward, 1.0 / size);
        let facing = dot(outward, direction);
        let distance = dot(outward, sub(origin, *start));
        if facing.abs() < 1e-12 {
            if distance > 0.0 {
                return None;
            }
            continue;
        }
        let t = -distance / facing;
        if facing < 0.0 {
            if t > enter {
                enter = t;
                normal = outward;
            }
        } else {
            exit = exit.min(t);
        }
    }
    (enter <= exit && (0.0..=1.0).contains(&enter)).then_some((enter, normal))
}

fn ray_ellipse(radii: Point, origin: Point, direction: Point) -> Option<(f64, Point)> {
    let o = [origin[0] / radii[0], origin[1] / radii[1]];
    let d = [direction[0] / radii[0], direction[1] / radii[1]];
    let a = dot(d, d);
    let c = dot(o, o) - 1.0;
    if a == 0.0 || c <= 0.0 {
        return None;
    }
    let b = 2.0 * dot(o, d);
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }
    let t = (-b - discriminant.sqrt()) / (2.0 * a);
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    let hit = add(origin, scale(direction, t));
    let gradient = [hit[0] / (radii[0] * radii[0]), hit[1] / (radii[1] * radii[1])];
    let size = length(gradient);
    Some((t, if size > 0.0 { scale(gradient, 1.0 / size) } else { [0.0, 0.0] }))
}

fn rectangle(center: Point, size: Point, rotation: f64) -> [Point; 4] {
    let (sin, cos) = rotation.sin_cos();
    let half = [size[0].abs() * 0.5, size[1].abs() * 0.5];
    [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]
        .map(|corner| add(center, rotate([corner[0] * half[0], corner[1] * half[1]], cos, sin)))
}

pub fn test(collider: &Collider, query: &Query) -> Option<Hit> {
    if !collider.usable() {
        return None;
    }
    let hit = |distance: f64, position: Point, normal: Point| Hit {
        id: collider.id,
        distance,
        position,
        normal,
    };
    let polygon = collider.polygon();
    let radii = collider.radii();
    match *query {
        Query::Point(point) => {
            let local = collider.to_local(point);
            let inside = match &polygon {
                Some(polygon) => inside_polygon(polygon, local),
                None => inside_ellipse(radii, local),
            };
            inside.then(|| hit(0.0, point, [0.0, 0.0]))
        }
        Query::Area { center, size, rotation } => {
            if !(finite(center) && finite(size) && rotation.is_finite()) {
                return None;
            }
            let area = rectangle(center, size, rotation).map(|corner| collider.to_local(corner));
            let overlaps = match &polygon {
                Some(polygon) => polygons_overlap(polygon, &area),
                None => ellipse_polygon_overlap(radii, &area),
            };
            overlaps.then(|| hit(0.0, center, [0.0, 0.0]))
        }
        Query::Radius { center, radius } => {
            if !(finite(center) && radius.is_finite() && radius >= 0.0) {
                return None;
            }
            let local = collider.to_local(center);
            let overlaps = match &polygon {
                Some(polygon) => inside_polygon(polygon, local) || edge_distance(polygon, local) <= radius,
                None => {
                    inside_ellipse(radii, local) || length(sub(local, closest_on_ellipse(radii, local))) <= radius
                }
            };
            overlaps.then(|| hit(0.0, center, [0.0, 0.0]))
        }
        Query::Ray { origin, direction } => {
            if !(finite(origin) && finite(direction)) || direction == [0.0, 0.0] {
                return None;
            }
            let (sin, cos) = collider.rotation.sin_cos();
            let local_origin = collider.to_local(origin);
            let local_direction = rotate(direction, cos, -sin);
            let (t, normal) = match &polygon {
                Some(polygon) => ray_polygon(polygon, local_origin, local_direction)?,
                None => ray_ellipse(radii, local_origin, local_direction)?,
            };
            Some(hit(
                t * length(direction),
                add(origin, scale(direction, t)),
                rotate(normal, cos, sin),
            ))
        }
    }
}

pub fn run<'a>(colliders: impl IntoIterator<Item = &'a Collider>, query: &Query) -> Vec<Hit> {
    colliders.into_iter().filter_map(|collider| test(collider, query)).collect()
}

pub const NO_SHAPE: u32 = u32::MAX;
pub const NO_OBJECT: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    Renderable = 1,
    Shape = 2,
    Image = 3,
    Text = 4,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuObject {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub anchor: [f32; 2],
    pub rotation: f32,
    pub shape: u32,
    pub color: [f32; 4],
    pub kind: u32,
    pub z_index: f32,
    pub id_low: u32,
    pub id_high: u32,
}

impl GpuObject {
    pub const EMPTY: GpuObject = GpuObject {
        position: [0.0; 2],
        size: [0.0; 2],
        anchor: [0.0; 2],
        rotation: 0.0,
        shape: NO_SHAPE,
        color: [0.0; 4],
        kind: 0,
        z_index: 0.0,
        id_low: 0,
        id_high: 0,
    };

    pub fn new(id: u64, kind: ObjectKind, collider: Option<&Collider>, color: [f32; 4], z_index: f64) -> Self {
        let mut object = GpuObject {
            color,
            kind: kind as u32,
            z_index: z_index as f32,
            id_low: id as u32,
            id_high: (id >> 32) as u32,
            ..Self::EMPTY
        };
        if let Some(collider) = collider {
            object.position = collider.position.map(|value| value as f32);
            object.size = collider.size.map(|value| value as f32);
            object.anchor = collider.anchor.map(|value| value as f32);
            object.rotation = collider.rotation as f32;
            object.shape = collider.shape.index();
        }
        object
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuQuery {
    pub kind: u32,
    pub count: u32,
    pub capacity: u32,
    pub padding: u32,
    pub first: [f32; 4],
    pub second: [f32; 4],
}

impl GpuQuery {
    pub fn new(query: &Query, count: u32) -> Self {
        let narrow = |values: [f64; 4]| values.map(|value| value as f32);
        let (kind, first, second) = match *query {
            Query::Point(point) => (0, [point[0], point[1], 0.0, 0.0], [0.0; 4]),
            Query::Area { center, size, rotation } => {
                let (sin, cos) = rotation.sin_cos();
                (1, [center[0], center[1], size[0].abs(), size[1].abs()], [cos, sin, 0.0, 0.0])
            }
            Query::Radius { center, radius } => (2, [center[0], center[1], radius, 0.0], [0.0; 4]),
            Query::Ray { origin, direction } => (3, [origin[0], origin[1], direction[0], direction[1]], [0.0; 4]),
        };
        Self {
            kind,
            count,
            capacity: count,
            padding: 0,
            first: narrow(first),
            second: narrow(second),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuHit {
    pub id_low: u32,
    pub id_high: u32,
    pub distance: f32,
    pub padding: u32,
    pub position: [f32; 2],
    pub normal: [f32; 2],
}

impl GpuHit {
    pub fn hit(&self) -> Hit {
        Hit {
            id: u64::from(self.id_low) | (u64::from(self.id_high) << 32),
            distance: f64::from(self.distance),
            position: self.position.map(f64::from),
            normal: self.normal.map(f64::from),
        }
    }
}

pub fn valid_query(query: &Query) -> bool {
    match *query {
        Query::Point(point) => finite(point),
        Query::Area { center, size, rotation } => finite(center) && finite(size) && rotation.is_finite(),
        Query::Radius { center, radius } => finite(center) && radius.is_finite() && radius >= 0.0,
        Query::Ray { origin, direction } => finite(origin) && finite(direction) && direction != [0.0, 0.0],
    }
}
