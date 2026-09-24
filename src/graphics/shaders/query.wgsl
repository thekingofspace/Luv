struct Query {
    kind: u32,
    count: u32,
    capacity: u32,
    padding: u32,
    first: vec4<f32>,
    second: vec4<f32>,
}

struct Hit {
    id_low: u32,
    id_high: u32,
    distance: f32,
    padding: u32,
    position: vec2<f32>,
    normal: vec2<f32>,
}

struct Hits {
    count: atomic<u32>,
    padding0: u32,
    padding1: u32,
    padding2: u32,
    items: array<Hit>,
}

@group(1) @binding(0) var<uniform> query: Query;
@group(1) @binding(1) var<storage, read_write> hits: Hits;

const MISS: f32 = -1.0;
const PARALLEL: f32 = 1e-9;
const FAR: f32 = 1e30;

fn record(object: Object, distance: f32, position: vec2<f32>, normal: vec2<f32>) {
    let index = atomicAdd(&hits.count, 1u);
    if index < query.capacity {
        hits.items[index] = Hit(object.id_low, object.id_high, distance, 0u, position, normal);
    }
}

fn cross2(a: vec2<f32>, b: vec2<f32>) -> f32 {
    return a.x * b.y - a.y * b.x;
}

fn segment_distance(point: vec2<f32>, start: vec2<f32>, end: vec2<f32>) -> f32 {
    let edge = end - start;
    let squared = dot(edge, edge);
    var t = 0.0;
    if squared > 0.0 {
        t = clamp(dot(point - start, edge) / squared, 0.0, 1.0);
    }
    return length(point - (start + edge * t));
}

fn inside_ellipse(radii: vec2<f32>, point: vec2<f32>) -> bool {
    let scaled = point / radii;
    return dot(scaled, scaled) <= 1.0;
}

fn closest_on_ellipse(radii: vec2<f32>, point: vec2<f32>) -> vec2<f32> {
    let a = radii.x;
    let b = radii.y;
    let p = abs(point);
    var t = vec2<f32>(0.70710678, 0.70710678);
    for (var step = 0; step < 4; step++) {
        let x = a * t.x;
        let y = b * t.y;
        let ex = (a * a - b * b) * t.x * t.x * t.x / a;
        let ey = (b * b - a * a) * t.y * t.y * t.y / b;
        let r = length(vec2<f32>(x - ex, y - ey));
        let q = max(length(vec2<f32>(p.x - ex, p.y - ey)), 1e-12);
        t = clamp(vec2<f32>(((p.x - ex) * r / q + ex) / a, ((p.y - ey) * r / q + ey) / b), vec2<f32>(0.0), vec2<f32>(1.0));
        t = t / max(length(t), 1e-12);
    }
    return vec2<f32>(a * t.x, b * t.y) * select(vec2<f32>(-1.0), vec2<f32>(1.0), point >= vec2<f32>(0.0));
}

fn project_shape(shape: u32, count: u32, size: vec2<f32>, axis: vec2<f32>) -> vec2<f32> {
    var low = 1e30;
    var high = -1e30;
    for (var index = 0u; index < count; index++) {
        let value = dot(shape_point(shape, index, size), axis);
        low = min(low, value);
        high = max(high, value);
    }
    return vec2<f32>(low, high);
}

fn project_quad(quad: ptr<function, array<vec2<f32>, 4>>, axis: vec2<f32>) -> vec2<f32> {
    var low = 1e30;
    var high = -1e30;
    for (var index = 0u; index < 4u; index++) {
        let value = dot((*quad)[index], axis);
        low = min(low, value);
        high = max(high, value);
    }
    return vec2<f32>(low, high);
}

fn separated(first: vec2<f32>, second: vec2<f32>) -> bool {
    return first.y < second.x || second.y < first.x;
}

fn polygon_sdf(shape: u32, size: vec2<f32>, point: vec2<f32>) -> f32 {
    let count = shape_range(shape).y;
    if count < 3u {
        return FAR;
    }
    var nearest = FAR;
    var sign = 1.0;
    var previous = shape_point(shape, count - 1u, size);
    for (var index = 0u; index < count; index++) {
        let current = shape_point(shape, index, size);
        let edge = previous - current;
        let offset = point - current;
        let squared = dot(edge, edge);
        var along = 0.0;
        if squared > 0.0 {
            along = clamp(dot(offset, edge) / squared, 0.0, 1.0);
        }
        let gap = offset - edge * along;
        nearest = min(nearest, dot(gap, gap));
        let crossing = vec3<bool>((point.y >= current.y), (point.y < previous.y), (edge.x * offset.y > edge.y * offset.x));
        if all(crossing) || all(!crossing) {
            sign = -sign;
        }
        previous = current;
    }
    return sign * sqrt(nearest);
}

fn segments_cross(a0: vec2<f32>, a1: vec2<f32>, b0: vec2<f32>, b1: vec2<f32>) -> bool {
    let run = a1 - a0;
    let span = b1 - b0;
    let gap = b0 - a0;
    let denominator = cross2(run, span);
    if abs(denominator) <= PARALLEL {
        if abs(cross2(gap, run)) > PARALLEL {
            return false;
        }
        let squared = dot(run, run);
        if squared <= 0.0 {
            return false;
        }
        let start = dot(gap, run) / squared;
        let end = start + dot(span, run) / squared;
        return min(start, end) <= 1.0 && max(start, end) >= 0.0;
    }
    let along = cross2(gap, span) / denominator;
    let across = cross2(gap, run) / denominator;
    return along >= 0.0 && along <= 1.0 && across >= 0.0 && across <= 1.0;
}

fn inside_quad(quad: ptr<function, array<vec2<f32>, 4>>, point: vec2<f32>) -> bool {
    var inside = false;
    var previous = (*quad)[3];
    for (var index = 0u; index < 4u; index++) {
        let current = (*quad)[index];
        let edge = previous - current;
        let offset = point - current;
        let crossing = vec3<bool>((point.y >= current.y), (point.y < previous.y), (edge.x * offset.y > edge.y * offset.x));
        if all(crossing) || all(!crossing) {
            inside = !inside;
        }
        previous = current;
    }
    return inside;
}

fn area_polygon(shape: u32, size: vec2<f32>, quad: ptr<function, array<vec2<f32>, 4>>) -> bool {
    let count = shape_range(shape).y;
    for (var index = 0u; index < count; index++) {
        let start = shape_point(shape, index, size);
        let end = shape_point(shape, (index + 1u) % count, size);
        for (var other = 0u; other < 4u; other++) {
            if segments_cross(start, end, (*quad)[other], (*quad)[(other + 1u) % 4u]) {
                return true;
            }
        }
    }
    for (var index = 0u; index < count; index++) {
        if inside_quad(quad, shape_point(shape, index, size)) {
            return true;
        }
    }
    for (var index = 0u; index < 4u; index++) {
        if polygon_sdf(shape, size, (*quad)[index]) <= 0.0 {
            return true;
        }
    }
    return false;
}

fn area_ellipse(radii: vec2<f32>, quad: ptr<function, array<vec2<f32>, 4>>) -> bool {
    var unit: array<vec2<f32>, 4>;
    for (var index = 0u; index < 4u; index++) {
        unit[index] = (*quad)[index] / radii;
    }
    var positive = false;
    var negative = false;
    for (var index = 0u; index < 4u; index++) {
        let start = unit[index];
        let side = cross2(unit[(index + 1u) % 4u] - start, -start);
        if side > 0.0 {
            positive = true;
        } else if side < 0.0 {
            negative = true;
        }
    }
    if !(positive && negative) {
        return true;
    }
    for (var index = 0u; index < 4u; index++) {
        if segment_distance(vec2<f32>(0.0), unit[index], unit[(index + 1u) % 4u]) <= 1.0 {
            return true;
        }
    }
    return false;
}

fn ray_polygon(shape: u32, size: vec2<f32>, origin: vec2<f32>, direction: vec2<f32>) -> vec3<f32> {
    let count = shape_range(shape).y;
    if count < 3u || polygon_sdf(shape, size, origin) <= 0.0 {
        return vec3<f32>(MISS, 0.0, 0.0);
    }
    var area = 0.0;
    for (var index = 0u; index < count; index++) {
        area += cross2(shape_point(shape, index, size), shape_point(shape, (index + 1u) % count, size));
    }
    var nearest = FAR;
    var normal = vec2<f32>(0.0);
    for (var index = 0u; index < count; index++) {
        let start = shape_point(shape, index, size);
        let edge = shape_point(shape, (index + 1u) % count, size) - start;
        var outward = vec2<f32>(-edge.y, edge.x);
        if area > 0.0 {
            outward = vec2<f32>(edge.y, -edge.x);
        }
        let magnitude = length(outward);
        if magnitude == 0.0 {
            continue;
        }
        outward = outward / magnitude;
        if dot(outward, direction) >= 0.0 {
            continue;
        }
        let denominator = cross2(direction, edge);
        if abs(denominator) <= PARALLEL {
            continue;
        }
        let gap = start - origin;
        let along = cross2(gap, edge) / denominator;
        let across = cross2(gap, direction) / denominator;
        if along < 0.0 || along > 1.0 || across < 0.0 || across > 1.0 {
            continue;
        }
        if along < nearest {
            nearest = along;
            normal = outward;
        }
    }
    if nearest <= 1.0 {
        return vec3<f32>(nearest, normal);
    }
    return vec3<f32>(MISS, 0.0, 0.0);
}

fn ray_ellipse(radii: vec2<f32>, origin: vec2<f32>, direction: vec2<f32>) -> vec3<f32> {
    let o = origin / radii;
    let d = direction / radii;
    let a = dot(d, d);
    let c = dot(o, o) - 1.0;
    if a == 0.0 || c <= 0.0 {
        return vec3<f32>(MISS, 0.0, 0.0);
    }
    let b = 2.0 * dot(o, d);
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return vec3<f32>(MISS, 0.0, 0.0);
    }
    let t = (-b - sqrt(discriminant)) / (2.0 * a);
    if t < 0.0 || t > 1.0 {
        return vec3<f32>(MISS, 0.0, 0.0);
    }
    let hit = origin + direction * t;
    let gradient = hit / (radii * radii);
    let magnitude = length(gradient);
    var normal = vec2<f32>(0.0);
    if magnitude > 0.0 {
        normal = gradient / magnitude;
    }
    return vec3<f32>(t, normal);
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if index >= query.count {
        return;
    }
    let object = objects[index];
    if object.shape == NO_SHAPE || object.size.x == 0.0 || object.size.y == 0.0 {
        return;
    }
    let radii = abs(object.size) * 0.5;
    let circle = object.shape == SHAPE_CIRCLE;
    switch query.kind {
        case 0u: {
            let local = object_local(object, query.first.xy);
            var inside = false;
            if circle {
                inside = inside_ellipse(radii, local);
            } else {
                inside = shape_distance(object.shape, object.size, local) <= 0.0;
            }
            if inside {
                record(object, 0.0, query.first.xy, vec2<f32>(0.0));
            }
        }
        case 1u: {
            let half = query.first.zw * 0.5;
            var quad: array<vec2<f32>, 4>;
            quad[0] = object_local(object, query.first.xy + rotate_by(vec2<f32>(-half.x, -half.y), query.second.xy));
            quad[1] = object_local(object, query.first.xy + rotate_by(vec2<f32>(half.x, -half.y), query.second.xy));
            quad[2] = object_local(object, query.first.xy + rotate_by(vec2<f32>(half.x, half.y), query.second.xy));
            quad[3] = object_local(object, query.first.xy + rotate_by(vec2<f32>(-half.x, half.y), query.second.xy));
            var overlaps = false;
            if circle {
                overlaps = area_ellipse(radii, &quad);
            } else {
                overlaps = area_polygon(object.shape, object.size, &quad);
            }
            if overlaps {
                record(object, 0.0, query.first.xy, vec2<f32>(0.0));
            }
        }
        case 2u: {
            let local = object_local(object, query.first.xy);
            let radius = query.first.z;
            var overlaps = false;
            if circle {
                overlaps = inside_ellipse(radii, local) || length(local - closest_on_ellipse(radii, local)) <= radius;
            } else {
                overlaps = shape_distance(object.shape, object.size, local) <= radius;
            }
            if overlaps {
                record(object, 0.0, query.first.xy, vec2<f32>(0.0));
            }
        }
        default: {
            let origin = query.first.xy;
            let direction = query.first.zw;
            let local_origin = object_local(object, origin);
            let local_direction = rotate2d(direction, -object.rotation);
            var result: vec3<f32>;
            if circle {
                result = ray_ellipse(radii, local_origin, local_direction);
            } else {
                result = ray_polygon(object.shape, object.size, local_origin, local_direction);
            }
            if result.x >= 0.0 {
                record(
                    object,
                    result.x * length(direction),
                    origin + direction * result.x,
                    rotate2d(result.yz, object.rotation),
                );
            }
        }
    }
}

fn rotate_by(value: vec2<f32>, cos_sin: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(value.x * cos_sin.x - value.y * cos_sin.y, value.x * cos_sin.y + value.y * cos_sin.x);
}
