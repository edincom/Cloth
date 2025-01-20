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

@compute @workgroup_size(64, 1, 1)
fn computeMain(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    
    if (index >= arrayLength(&instances_in)) {
        return;
    }

    var total_force = vec3<f32>(0.0, 0.0, 0.0);
    
    // Apply gravity
    let gravity = vec3<f32>(0.0, -9.81, 0.0);
    total_force += gravity * sim_params.mass;
    
    if (index == 0u) {
        debug_data.gravity_force = -9.81 * sim_params.mass;
    }

    // Calculate spring forces
    for (var i = 0u; i < arrayLength(&springs); i++) {
        let spring = springs[i];
        
        if (spring.instance_a == index || spring.instance_b == index) {
            let pos_a = instances_in[spring.instance_a].position.xyz;
            let pos_b = instances_in[spring.instance_b].position.xyz;
            
            let delta = pos_a - pos_b;
            let distance = length(delta);
            
            if (distance > 0.0) {
                let direction = delta / distance;
                let spring_force = -spring.stiffness * (distance - spring.rest_length) * direction;
                
                // Apply spring force
                if (spring.instance_a == index) {
                    total_force += spring_force;
                } else {
                    total_force -= spring_force;
                }
                
                // Store spring force for debugging (first instance only)
                if (index == 0u && i < 32u) {
                    debug_data.spring_forces[i] = length(spring_force);
                }
            }
        }
    }
    
    if (index == 0u) {
        debug_data.total_force = length(total_force);
    }

    // Apply damping
    let velocity = instances_in[index].speed.xyz;
    let damping_force = -sim_params.damping * velocity;
    total_force += damping_force;
    
    if (index == 0u) {
        debug_data.final_force = length(total_force);
    }

    // Update velocity
    let acceleration = total_force / sim_params.mass;
    let new_velocity = velocity + acceleration * sim_params.delta_time;
    
    // Update position
    let new_position = instances_in[index].position.xyz + new_velocity * sim_params.delta_time;
    
    // Write results
    instances_out[index].speed = vec4<f32>(new_velocity, 0.0);
    instances_out[index].position = vec4<f32>(new_position, 0.0);
}