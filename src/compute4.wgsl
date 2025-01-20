struct Instance {
    position: vec4<f32>,
    speed: vec4<f32>,
}

struct Spring {
    stiffness: f32,
    rest_length: f32,
    instance_a: u32,
    instance_b: u32,
}

struct SimParams {
    delta_time: f32,
    damping: f32,
    mass: f32,
}

struct DebugData {
    gravity_force: f32,
    spring_forces: array<f32, 32>,
    total_force: f32,
    final_force: f32,
}

@group(0) @binding(0) var<storage, read_write> instances_in: array<Instance>;
@group(0) @binding(1) var<storage, read_write> instances_out: array<Instance>;
@group(0) @binding(2) var<storage, read> springs: array<Spring>;
@group(0) @binding(3) var<uniform> sim_params: SimParams;
@group(0) @binding(4) var<storage, read_write> debug_data: DebugData;

const GRAVITY: vec3<f32> = vec3<f32>(0.0, -0.1, 0.0);
const SPHERE_RADIUS: f32 = 0.3;
const SPHERE_DAMPING: f32 = 0.3;

@compute @workgroup_size(64, 1, 1)
fn computeMain(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    
    if (index >= arrayLength(&instances_in)) {
        return;
    }

    var instance = instances_in[index];
    var total_force = GRAVITY * sim_params.mass;
    var total_spring_force = vec3<f32>(0.0);
    
    if (index == 0u) {
        debug_data.gravity_force = length(GRAVITY * sim_params.mass);
    }

    // Calculate spring forces
    for (var i = 0u; i < arrayLength(&springs); i++) {
        let spring = springs[i];
        
        if (spring.instance_a == index || spring.instance_b == index) {
            var other_index = select(spring.instance_a, spring.instance_b, spring.instance_a == index);
            let other_instance = instances_in[other_index];
            
            let displacement = other_instance.position.xyz - instance.position.xyz;
            let distance = length(displacement);
            
            if (distance > 0.0001) {
                let direction = displacement / distance;
                let extension = distance - spring.rest_length;
                
                let spring_force = direction * (spring.stiffness * extension);
                // let spring_force = direction * min(spring.stiffness * extension, 100.0); // Limit maximum force
                let final_spring_force = select(spring_force, -spring_force, spring.instance_b == index);
                
                total_spring_force += final_spring_force;
                total_force += final_spring_force;
                
                if (index == 0u && i < 32u) {
                    debug_data.spring_forces[i] = length(spring_force);
                }
            }
        }
    }

    if (index == 0u) {
        debug_data.total_force = length(total_spring_force);
    }

    // Apply damping
    let damping_force = -sim_params.damping * instance.speed.xyz;
    total_force += damping_force;
    
    if (index == 0u) {
        debug_data.final_force = length(total_force);
    }

    // Update velocity and position
    let acceleration = total_force / sim_params.mass;
    instance.speed = vec4<f32>(
        instance.speed.xyz + acceleration * sim_params.delta_time,
        instance.speed.w
    );
    
    instance.position = vec4<f32>(
        instance.position.xyz + instance.speed.xyz * sim_params.delta_time,
        instance.position.w
    );

    // Sphere collision handling
    let distance = length(instance.position.xyz);
    if (distance < SPHERE_RADIUS) {
        let normal = normalize(instance.position.xyz);
        // Push the particle slightly outside the sphere to prevent sticking
        instance.position = vec4<f32>(
            normal * (SPHERE_RADIUS + 0.001),
            instance.position.w
        );
        
        // Calculate reflection velocity with damping
        let dot_product = dot(instance.speed.xyz, normal);
        instance.speed = vec4<f32>(
            (instance.speed.xyz - 2.0 * dot_product * normal) * SPHERE_DAMPING,
            instance.speed.w
        );
    }

    instances_out[index] = instance;
}