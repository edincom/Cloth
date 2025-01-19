// compute.wgsl

struct Instance {
    position: vec4<f32>,
    speed: vec4<f32>,
};

struct Spring {
    instance_a: u32,
    instance_b: u32,
    rest_length: f32,
    stiffness: f32,
};

struct SimParams {
    delta_time: f32,
    damping: f32,
    mass: f32,
};


struct DebugData {
    gravity_force: f32,
    spring_forces: array<f32, 32>,  // Must use `array<>` instead of `[T; N]` in WGSL
    total_force: f32,
    final_force: f32,
};

// Instance storage buffers
@group(0) @binding(0) var<storage, read_write> instances_ping: array<Instance>;
@group(0) @binding(1) var<storage, read_write> instances_pong: array<Instance>;

@group(0) @binding(2) var<storage, read> springs: array<Spring>;
@group(0) @binding(3) var<uniform> params: SimParams;

@group(0) @binding(4) var<storage, read_write> debug_data: DebugData;

// Gravity constant (downward acceleration)
const GRAVITY: vec3<f32> = vec3<f32>(0.0, -9.81, 0.0); // m/s² (adjust as needed)

// Ground level
const GROUND_LEVEL: f32 = 0.0; // Define the ground level



@compute @workgroup_size(WORKGROUP_SIZE)
fn computeMain(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    if (index >= arrayLength(&instances_ping)) {
        return;
    }

    var instance = instances_ping[index];
    var force: vec3<f32> = GRAVITY * params.mass;
    var total_spring_force: vec3<f32> = vec3<f32>(0.0, 0.0, 0.0);

    if (index == 0u) {
        debug_data.gravity_force = length(GRAVITY * params.mass);
    }

    for (var i: u32 = 0u; i < arrayLength(&springs); i++) {
        let spring = springs[i];

        if (spring.instance_a == index || spring.instance_b == index) {
            var other_index: u32;
            if (spring.instance_a == index) {
                other_index = spring.instance_b;
            } else {
                other_index = spring.instance_a;
            }

            let other_instance = instances_ping[other_index];
            let displacement = other_instance.position.xyz - instance.position.xyz;
            let current_length = length(displacement);
            
            if (current_length > 0.0001) {  // Avoid division by zero
                let direction = displacement / current_length;
                let extension = current_length - spring.rest_length;
                
                // Spring force calculation
                let spring_force = direction * (spring.stiffness * extension);
                
                // If this is instance_b, reverse the force direction
                let final_spring_force = select(spring_force, -spring_force, spring.instance_b == index);
                
                total_spring_force += final_spring_force;
                force += final_spring_force;

                if (index == 0u && i < 32u) {
                    debug_data.spring_forces[i] = length(spring_force);
                }
            }
        }
    }

    if (index == 0u) {
        // Store the magnitude of the total force (which includes both gravity and spring forces)
        debug_data.total_force = length(total_spring_force);  // This now stores just the spring force total
    }

    // Apply damping force
    force -= params.damping * instance.speed.xyz;

    if (index == 0u) {
        debug_data.final_force = length(force);
    }

    // Update velocity and position
    let acceleration = force / params.mass;
    
    // Fixed vector assignments
    instance.speed = vec4<f32>(
        instance.speed.xyz + acceleration * params.delta_time,
        instance.speed.w
    );
    
    instance.position = vec4<f32>(
        instance.position.xyz + instance.speed.xyz * params.delta_time,
        instance.position.w
    );

    // Sphere collision handling
    let distance = length(instance.position.xyz);
    let sphere_radius = 0.3;

    if (distance < sphere_radius) {
        let normal = normalize(instance.position.xyz);
        instance.position = vec4<f32>(normal * (sphere_radius + 0.001), instance.position.w);
        
        let damping = 0.3;
        let dot_product = dot(instance.speed.xyz, normal);
        instance.speed = vec4<f32>(
            (instance.speed.xyz - 2.0 * dot_product * normal) * damping,
            instance.speed.w
        );
    }

    instances_pong[index] = instance;
}