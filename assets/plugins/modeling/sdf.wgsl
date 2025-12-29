#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::mesh_bindings

struct SdfMaterial {
    color: vec4<f32>,
    time: f32,
};

@group(2) @binding(0)
var<uniform> material: SdfMaterial;

// --- SDF Functions ---

fn sdSphere(p: vec3<f32>, s: f32) -> f32 {
    return length(p) - s;
}

fn sdBox(p: vec3<f32>, b: vec3<f32>) -> f32 {
    let q = abs(p) - b;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

// --- Scene ---

fn map(p: vec3<f32>) -> f32 {
    let sphere = sdSphere(p, 1.0);
    let offset = vec3<f32>(sin(material.time) * 1.5, 0.0, 0.0);
    let box = sdBox(p - offset, vec3<f32>(0.75));
    
    // Smooth Union
    let k = 0.5;
    let h = max(k - abs(sphere - box), 0.0) / k;
    return min(sphere, box) - h * h * k * (1.0 / 4.0);
}

// --- Normal calculation ---

fn calcNormal(p: vec3<f32>) -> vec3<f32> {
    let e = vec2<f32>(0.001, 0.0);
    return normalize(vec3<f32>(
        map(p + e.xyy) - map(p - e.xyy),
        map(p + e.yxy) - map(p - e.yxy),
        map(p + e.yyx) - map(p - e.yyx)
    ));
}

@fragment
fn fragment(
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
) -> @location(0) vec4<f32> {
    let view_pos = view.world_position;
    let ray_origin = view_pos;
    let ray_dir = normalize(world_position.xyz - view_pos);

    var t = 0.0;
    let t_max = 20.0;
    var hit = false;
    var p = ray_origin;

    // Raymarching loop
    for (var i = 0; i < 128; i++) {
        p = ray_origin + ray_dir * t;
        let d = map(p);
        if (d < 0.001) {
            hit = true;
            break;
        }
        t += d;
        if (t > t_max) { break; }
    }

    if (hit) {
        let normal = calcNormal(p);
        let light_dir = normalize(vec3<f32>(5.0, 5.0, 5.0));
        let diff = max(dot(normal, light_dir), 0.1);
        let color = material.color.rgb * diff;
        return vec4<f32>(color, 1.0);
    } else {
        discard;
    }
}
