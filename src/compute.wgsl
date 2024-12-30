// compute.wgsl



struct Instance {
    position: vec4<f32>,
    speed: vec4<f32>,
};

// Instance storage buffers
@group(0) @binding(0) var<storage, read_write> instances_ping: array<Instance>;
@group(0) @binding(1) var<storage, read_write> instances_pong: array<Instance>;

struct TimeUniform {
    generation_duration: f32,
};

@group(0) @binding(2) var<uniform> time: TimeUniform;

// Gravity constant (downward acceleration)
const GRAVITY: f32 = -9.8; // m/s² (adjust as needed)

//Ground level
const GROUND_LEVEL: f32 = 0.0; // Define the ground level

// Compute shader entry point
@compute @workgroup_size(WORKGROUP_SIZE)
fn computeMain(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    var instance = instances_ping[index];

    // Since we're updating more frequently, we use the actual delta time
    let delta_time = time.generation_duration;

    // Update velocity (using real physics equations)
    instance.speed[1] += GRAVITY * 0.016;

    // Update position (using real physics equations)
    instance.position[0] += instance.speed[0] * 0.016;
    instance.position[1] += instance.speed[1] * 0.016;
    instance.position[2] += instance.speed[2] * 0.016;

    // Sphere collision check (adjusted for more realistic behavior)
    let distance = length(instance.position.xyz);
    let sphere_radius = 0.3;
    
    if (distance < sphere_radius) {
        // Move the point back to the surface of the sphere
        let normal = normalize(instance.position.xyz);
        instance.position.x = normal.x * sphere_radius;
        instance.position.y = normal.y * sphere_radius;
        instance.position.z = normal.z * sphere_radius;

        // Calculate reflection with energy loss (add damping)
        let damping = 0.8; // 0.8 = 80% energy preservation
        let dot_product = dot(instance.speed.xyz, normal);
        instance.speed.x = (instance.speed.x - 2.0 * dot_product * normal.x) * damping;
        instance.speed.y = (instance.speed.y - 2.0 * dot_product * normal.y) * damping;
        instance.speed.z = (instance.speed.z - 2.0 * dot_product * normal.z) * damping;
    }

    instances_pong[index] = instance;
}