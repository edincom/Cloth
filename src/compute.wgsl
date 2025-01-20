struct Instance {
    position: vec4<f32>,
    speed: vec4<f32>,
};

struct TimeUniform {
    generation_duration: f32,
};

struct PhysicsParams {
    structural_k: f32,
    shear_k: f32,
    bend_k: f32,
    damping: f32,
    mass: f32,
    rest_length: f32,
    dt: f32,
    padding: f32,
};

@group(0) @binding(0) var<storage, read_write> instances_ping: array<Instance>;
@group(0) @binding(1) var<storage, read_write> instances_pong: array<Instance>;
@group(0) @binding(2) var<uniform> time: TimeUniform;
@group(0) @binding(3) var<uniform> physics: PhysicsParams;

const GRAVITY: f32 = -9.81;

fn calculate_spring_force(pos1: vec3<f32>, pos2: vec3<f32>, rest_length: f32, k: f32) -> vec3<f32> {
    let delta = pos2 - pos1;
    let current_length = length(delta);
    let direction = normalize(delta);
    return k * (current_length - rest_length) * direction;
}

@compute @workgroup_size(WORKGROUP_SIZE)
fn computeMain(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    var instance = instances_ping[index];

    // Calcul de la ligne et colonne dans la grille
    let grid_size = u32(sqrt(f32(arrayLength(&instances_ping))));
    let row = index / grid_size;
    let col = index % grid_size;

    // Fixer les points du bord supérieur
    if (row == 0) {
        instances_pong[index] = instance;
        return;
    }

    // Position actuelle du point
    let pos = instance.position.xyz;
    var total_force = vec3<f32>(0.0, 0.0, 0.0);

    // Forces structurelles (voisins directs)
    // Voisin gauche
    if (col > 0) {
        let left_index = index - 1;
        let left_pos = instances_ping[left_index].position.xyz;
        total_force += calculate_spring_force(pos, left_pos, physics.rest_length, physics.structural_k);
    }

    // Voisin droit
    if (col < grid_size - 1) {
        let right_index = index + 1;
        let right_pos = instances_ping[right_index].position.xyz;
        total_force += calculate_spring_force(pos, right_pos, physics.rest_length, physics.structural_k);
    }

    // Voisin haut
    if (row > 0) {
        let up_index = index - grid_size;
        let up_pos = instances_ping[up_index].position.xyz;
        total_force += calculate_spring_force(pos, up_pos, physics.rest_length, physics.structural_k);
    }

    // Voisin bas
    if (row < grid_size - 1) {
        let down_index = index + grid_size;
        let down_pos = instances_ping[down_index].position.xyz;
        total_force += calculate_spring_force(pos, down_pos, physics.rest_length, physics.structural_k);
    }

    // Forces de cisaillement (diagonales)
    // Diagonale haut-gauche
    if (row > 0 && col > 0) {
        let diag_index = index - grid_size - 1;
        let diag_pos = instances_ping[diag_index].position.xyz;
        total_force += calculate_spring_force(pos, diag_pos, physics.rest_length * 1.414, physics.shear_k);
    }

    // Diagonale haut-droite
    if (row > 0 && col < grid_size - 1) {
        let diag_index = index - grid_size + 1;
        let diag_pos = instances_ping[diag_index].position.xyz;
        total_force += calculate_spring_force(pos, diag_pos, physics.rest_length * 1.414, physics.shear_k);
    }

    // Diagonale bas-gauche
    if (row < grid_size - 1 && col > 0) {
        let diag_index = index + grid_size - 1;
        let diag_pos = instances_ping[diag_index].position.xyz;
        total_force += calculate_spring_force(pos, diag_pos, physics.rest_length * 1.414, physics.shear_k);
    }

    // Diagonale bas-droite
    if (row < grid_size - 1 && col < grid_size - 1) {
        let diag_index = index + grid_size + 1;
        let diag_pos = instances_ping[diag_index].position.xyz;
        total_force += calculate_spring_force(pos, diag_pos, physics.rest_length * 1.414, physics.shear_k);
    }

    // Forces de flexion
    if (col > 1) {
        let bend_index = index - 2;
        let bend_pos = instances_ping[bend_index].position.xyz;
        total_force += calculate_spring_force(pos, bend_pos, physics.rest_length * 2.0, physics.bend_k);
    }

    // Flexion horizontale droite
    if (col < grid_size - 2) {
        let bend_index = index + 2;
        let bend_pos = instances_ping[bend_index].position.xyz;
        total_force += calculate_spring_force(pos, bend_pos, physics.rest_length * 2.0, physics.bend_k);
    }

    // Flexion verticale haut
    if (row > 1) {
        let bend_index = index - (grid_size * 2);
        let bend_pos = instances_ping[bend_index].position.xyz;
        total_force += calculate_spring_force(pos, bend_pos, physics.rest_length * 2.0, physics.bend_k);
    }

    // Flexion verticale bas
    if (row < grid_size - 2) {
        let bend_index = index + (grid_size * 2);
        let bend_pos = instances_ping[bend_index].position.xyz;
        total_force += calculate_spring_force(pos, bend_pos, physics.rest_length * 2.0, physics.bend_k);
    }

    // Force d'amortissement
    let damping_force = -physics.damping * instance.speed.xyz;
    total_force += damping_force;

    // Gravité
    total_force += vec3<f32>(0.0, GRAVITY * physics.mass, 0.0);

    // Sphère collision avec friction
    let sphere_radius = 0.3;
    let distance = length(instance.position.xyz);
    
    if (distance < sphere_radius) {
        // Normal vector from sphere center to point
        let normal = normalize(instance.position.xyz);
        
        // Repositionnement sur la surface de la sphère
        instance.position.x = normal.x * sphere_radius;
        instance.position.y = normal.y * sphere_radius;
        instance.position.z = normal.z * sphere_radius;

        // Calcul des forces pour la friction
        let Ro = total_force;
        let In = normal;
        
        // Composante normale de la force (Ro.n)
        let Ro_n_magnitude = dot(Ro, In);
        let Ro_n = In * Ro_n_magnitude;
        
        // Composante tangentielle de la force (Ro.t)
        let Ro_t = Ro - Ro_n;
        let Ro_t_magnitude = length(Ro_t);
        
        // Si la composante tangentielle n'est pas nulle
        if (Ro_t_magnitude > 0.0001) {
            // Direction de la force tangentielle
            let It = Ro_t / Ro_t_magnitude;
            
            // Coefficient de friction
            let cf = 0.5; // Ajustez cette valeur selon vos besoins
            
            // Force de friction
            let friction_magnitude = min(Ro_t_magnitude, cf * abs(Ro_n_magnitude));
            let friction_force = -friction_magnitude * It;
            
            // Ajout de la force de friction à la force totale
            total_force += friction_force;
        }

        // Réflexion de la vitesse avec amortissement
        let damping = 0.8;
        let dot_product = dot(instance.speed.xyz, normal);
        instance.speed.x = (instance.speed.x - 2.0 * dot_product * normal.x) * damping;
        instance.speed.y = (instance.speed.y - 2.0 * dot_product * normal.y) * damping;
        instance.speed.z = (instance.speed.z - 2.0 * dot_product * normal.z) * damping;
    }

    // Mise à jour de la vitesse
    let acceleration = total_force / physics.mass;
    instance.speed.x += acceleration.x * physics.dt;
    instance.speed.y += acceleration.y * physics.dt;
    instance.speed.z += acceleration.z * physics.dt;

    // Mise à jour de la position
    instance.position.x += instance.speed.x * physics.dt;
    instance.position.y += instance.speed.y * physics.dt;
    instance.position.z += instance.speed.z * physics.dt;

    instances_pong[index] = instance;
}