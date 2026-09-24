use super::scene::AlphaMask;
use crate::graphics::geometry::{Collider, Hit, Query};

const MAX_STEPS: u32 = 4096;
const MAX_SCAN: u32 = 256;

fn texel(mask: &AlphaMask, uv: [f32; 4], collider: &Collider, local: [f64; 2]) -> u8 {
    let width = collider.size[0];
    let height = collider.size[1];
    if width == 0.0 || height == 0.0 {
        return 0;
    }
    let across = (local[0] / width + 0.5).clamp(0.0, 1.0);
    let down = (local[1] / height + 0.5).clamp(0.0, 1.0);
    let u = f64::from(uv[0]) + across * (f64::from(uv[2]) - f64::from(uv[0]));
    let v = f64::from(uv[1]) + down * (f64::from(uv[3]) - f64::from(uv[1]));
    let x = (u * f64::from(mask.width)).floor().clamp(0.0, f64::from(mask.width.max(1) - 1));
    let y = (v * f64::from(mask.height)).floor().clamp(0.0, f64::from(mask.height.max(1) - 1));
    mask.at(x as u32, y as u32)
}

fn opaque(mask: &AlphaMask, threshold: f64, uv: [f32; 4], collider: &Collider, local: [f64; 2]) -> bool {
    f64::from(texel(mask, uv, collider, local)) / 255.0 >= threshold
}

fn span(uv: [f32; 4], mask: &AlphaMask, collider: &Collider) -> f64 {
    let across = (f64::from(uv[2]) - f64::from(uv[0])).abs() * f64::from(mask.width);
    let down = (f64::from(uv[3]) - f64::from(uv[1])).abs() * f64::from(mask.height);
    let x = if across > 0.0 { collider.size[0].abs() / across } else { collider.size[0].abs() };
    let y = if down > 0.0 { collider.size[1].abs() / down } else { collider.size[1].abs() };
    x.min(y).max(1e-3)
}

fn edges(collider: &Collider, origin: [f64; 2], direction: [f64; 2]) -> Option<(f64, f64)> {
    let half = [collider.size[0].abs() * 0.5, collider.size[1].abs() * 0.5];
    let mut enter = 0.0f64;
    let mut exit = 1.0f64;
    for axis in 0..2 {
        let start = origin[axis];
        let step = direction[axis];
        if step.abs() < 1e-12 {
            if start < -half[axis] || start > half[axis] {
                return None;
            }
            continue;
        }
        let first = (-half[axis] - start) / step;
        let second = (half[axis] - start) / step;
        let (low, high) = if first < second { (first, second) } else { (second, first) };
        enter = enter.max(low);
        exit = exit.min(high);
    }
    (enter <= exit).then_some((enter, exit))
}

pub fn refine(
    mask: &AlphaMask,
    threshold: f64,
    uv: [f32; 4],
    collider: &Collider,
    query: &Query,
    hit: Hit,
) -> Option<Hit> {
    if mask.width == 0 || mask.height == 0 {
        return Some(hit);
    }
    match *query {
        Query::Point(point) => opaque(mask, threshold, uv, collider, collider.to_local(point)).then_some(hit),
        Query::Radius { center, radius } => {
            let local = collider.to_local(center);
            if opaque(mask, threshold, uv, collider, local) {
                return Some(hit);
            }
            let step = span(uv, mask, collider);
            let reach = (radius / step).ceil().min(f64::from(MAX_SCAN)) as i32;
            for row in -reach..=reach {
                for column in -reach..=reach {
                    let offset = [f64::from(column) * step, f64::from(row) * step];
                    if offset[0] * offset[0] + offset[1] * offset[1] > radius * radius {
                        continue;
                    }
                    let probe = [local[0] + offset[0], local[1] + offset[1]];
                    if opaque(mask, threshold, uv, collider, probe) {
                        return Some(hit);
                    }
                }
            }
            None
        }
        Query::Area { .. } => {
            let step = span(uv, mask, collider);
            let half = [collider.size[0].abs() * 0.5, collider.size[1].abs() * 0.5];
            let columns = ((half[0] * 2.0 / step).ceil() as u32).clamp(1, MAX_SCAN);
            let rows = ((half[1] * 2.0 / step).ceil() as u32).clamp(1, MAX_SCAN);
            for row in 0..rows {
                for column in 0..columns {
                    let x = -half[0] + (f64::from(column) + 0.5) * (half[0] * 2.0 / f64::from(columns));
                    let y = -half[1] + (f64::from(row) + 0.5) * (half[1] * 2.0 / f64::from(rows));
                    let local = [x, y];
                    if !opaque(mask, threshold, uv, collider, local) {
                        continue;
                    }
                    if geometry_contains(collider, query, local) {
                        return Some(hit);
                    }
                }
            }
            None
        }
        Query::Ray { origin, direction } => {
            let (sin, cos) = collider.rotation.sin_cos();
            let local_origin = collider.to_local(origin);
            let local_direction = [
                direction[0] * cos + direction[1] * sin,
                -direction[0] * sin + direction[1] * cos,
            ];
            let (enter, exit) = edges(collider, local_origin, local_direction)?;
            let length = (local_direction[0] * local_direction[0] + local_direction[1] * local_direction[1]).sqrt();
            if length == 0.0 {
                return None;
            }
            let step = (span(uv, mask, collider) / length).max(1e-6);
            let mut along = enter.max(0.0);
            let mut steps = 0;
            while along <= exit && steps < MAX_STEPS {
                let local = [
                    local_origin[0] + local_direction[0] * along,
                    local_origin[1] + local_direction[1] * along,
                ];
                if opaque(mask, threshold, uv, collider, local) {
                    let world = [origin[0] + direction[0] * along, origin[1] + direction[1] * along];
                    let reach = (direction[0] * direction[0] + direction[1] * direction[1]).sqrt();
                    return Some(Hit {
                        id: hit.id,
                        distance: along * reach,
                        position: world,
                        normal: hit.normal,
                    });
                }
                along += step;
                steps += 1;
            }
            None
        }
    }
}

fn geometry_contains(collider: &Collider, query: &Query, local: [f64; 2]) -> bool {
    let world = collider.to_world(local);
    let probe = Query::Point(world);
    crate::graphics::geometry::test(collider, &probe).is_some() && overlaps(query, world)
}

fn overlaps(query: &Query, point: [f64; 2]) -> bool {
    let Query::Area { center, size, rotation } = *query else {
        return true;
    };
    let (sin, cos) = rotation.sin_cos();
    let offset = [point[0] - center[0], point[1] - center[1]];
    let local = [offset[0] * cos + offset[1] * sin, -offset[0] * sin + offset[1] * cos];
    local[0].abs() <= size[0].abs() * 0.5 && local[1].abs() <= size[1].abs() * 0.5
}
