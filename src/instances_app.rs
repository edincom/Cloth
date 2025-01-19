use wgpu_bootstrap::{
    cgmath::{self, InnerSpace}, egui,
    util::{
        geometry::icosphere,
        orbit_camera::{CameraUniform, OrbitCamera},
    },
    wgpu::{self, util::DeviceExt},
    App, Context,
};
use std::time::{Duration, Instant};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // Position attribute
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // Normal attribute
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress, // Offset after position
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // Color attribute
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress, // Offset after position and normal
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    position: [f32; 4],
    speed: [f32; 4],
}

impl Instance {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                offset: 0,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32;3]>() as wgpu::BufferAddress,
                shader_location: 4,
                format: wgpu::VertexFormat::Float32x3,
                },
            ],
        
        }
    
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct DebugData {
    gravity_force: f32,
    spring_forces: [f32; 32],  // Assuming max 32 springs affecting the particle
    total_force: f32,
    final_force: f32,
}


#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Spring {
    stiffness: f32,
    rest_length: f32,
    instance_a: u32,
    instance_b: u32,
}

fn generate_structural_springs(grid_size: u32, spacing: f32) -> Vec<Spring> {
    let mut springs = Vec::new();
    
    // println!("Generating structural springs for grid size: {}", grid_size); // Debug line
    
    for row in 0..grid_size {
        for col in 0..grid_size {
            let current = row * grid_size + col;
            
            // Horizontal spring
            if col < grid_size - 1 {
                springs.push(Spring {
                    instance_a: current,
                    instance_b: current + 1,
                    rest_length: spacing,
                    stiffness: 100.0,
                });
                //println!("Horizontal spring: {} <-> {}", current, current + 1); // Debug line
            }
            
            // Vertical spring
            if row < grid_size - 1 {
                springs.push(Spring {
                    instance_a: current,
                    instance_b: current + grid_size,
                    rest_length: spacing,
                    stiffness: 100.0,
                });
                //println!("Vertical spring: {} <-> {}", current, current + grid_size); // Debug line
            }
        }
    }
    println!("Total number of springs generated: {}", springs.len()); // Debug line
    springs
}

fn generate_springs(grid_size: u32, spacing: f32) -> Vec<Spring> {
    let mut springs = Vec::new();
    
    // Constants for spring types
    let structural_stiffness = 1000.0;
    let shear_stiffness = 800.0;  // Slightly less stiff than structural
    let bend_stiffness = 500.0;   // Even less stiff for bend springs
    
    // Calculate rest lengths
    let structural_rest = spacing;
    let shear_rest = spacing * 2.0_f32.sqrt();  // Diagonal distance
    let bend_rest = spacing * 2.0;              // Double spacing distance
    
    for row in 0..grid_size {
        for col in 0..grid_size {
            let current = row * grid_size + col;
            
            // Structural springs (horizontal and vertical)
            if col < grid_size - 1 {
                // Horizontal structural
                springs.push(Spring {
                    instance_a: current,
                    instance_b: current + 1,
                    rest_length: structural_rest,
                    stiffness: structural_stiffness,
                });
            }
            if row < grid_size - 1 {
                // Vertical structural
                springs.push(Spring {
                    instance_a: current,
                    instance_b: current + grid_size,
                    rest_length: structural_rest,
                    stiffness: structural_stiffness,
                });
            }
            
            // Shear springs (diagonal)
            if row < grid_size - 1 && col < grid_size - 1 {
                // Diagonal down-right
                springs.push(Spring {
                    instance_a: current,
                    instance_b: current + grid_size + 1,
                    rest_length: shear_rest,
                    stiffness: shear_stiffness,
                });
                // Diagonal down-left (from the right vertex)
                springs.push(Spring {
                    instance_a: current + 1,
                    instance_b: current + grid_size,
                    rest_length: shear_rest,
                    stiffness: shear_stiffness,
                });
            }
            
            // Bend springs (skip one particle)
            // Horizontal bend springs
            if col < grid_size - 2 {
                springs.push(Spring {
                    instance_a: current,
                    instance_b: current + 2,
                    rest_length: bend_rest,
                    stiffness: bend_stiffness,
                });
            }
            // Vertical bend springs
            if row < grid_size - 2 {
                springs.push(Spring {
                    instance_a: current,
                    instance_b: current + (grid_size * 2),
                    rest_length: bend_rest,
                    stiffness: bend_stiffness,
                });
            }
        }
    }
    
    println!("Generated springs: {} structural, {} shear, {} bend", 
             (grid_size - 1) * grid_size * 2,  // structural springs count
             (grid_size - 1) * (grid_size - 1) * 2,  // shear springs count
             (grid_size - 2) * grid_size * 2); // bend springs count
    
    springs
}


#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct TimeUniform {
    generation_duration: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct SimParams {
    delta_time: f32,
    damping: f32,
    mass: f32,
}

pub struct InstanceApp {
    vertex_buffer: wgpu::Buffer,
    instance_buffer: [wgpu::Buffer; 2],
    index_buffer: wgpu::Buffer,
    render_pipeline: wgpu::RenderPipeline,
    compute_pipeline: wgpu::ComputePipeline,
    num_indices: u32,
    num_instances: u32,
    camera: OrbitCamera,
    generation_duration: Duration,
    last_generation: Instant,
    bind_group: [wgpu::BindGroup; 2],
    sphere_index_buffer: wgpu::Buffer,
    sphere_vertex_buffer: wgpu::Buffer,
    num_sphere_indices: u32,
    sphere_render_pipeline: wgpu::RenderPipeline,
    debug_buffer : wgpu::Buffer,
}

fn generate_grid(
    context: &Context,
    rows: u32,
    cols: u32,
    spacing: f32,
    displacement: f32,
    sphere_scale: f32,
    sphere_color: [f32; 3],
) -> (Vec<Vertex>, wgpu::Buffer, Vec<Instance>, Vec<Instance>, Vec<u32>) {
    println!("Generating grid: rows = {}, cols = {}", rows, cols); // Debug line

    // Generate icosphere
    let (positions, indices) = icosphere(2);
    println!("Generated icosphere with {} vertices and {} indices", positions.len(), indices.len()); // Debug line

    // Create vertices
    let vertices: Vec<Vertex> = positions
        .iter()
        .map(|position| Vertex {
            position: (*position * sphere_scale).into(),
            normal: [0.0, 0.0, 0.0],
            color: sphere_color,
        })
        .collect();
    //println!("Created {} vertices", vertices.len()); // Debug line

    // Create index buffer
    let index_buffer = context
        .device()
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(indices.as_slice()),
            usage: wgpu::BufferUsages::INDEX,
        });

    // Generate grid of instances
    let instances: Vec<Instance> = (0..rows)
        .flat_map(|row| {
            (0..cols).map(move |col| {
                Instance {
                    position: [
                        (col as f32 - cols as f32 / 2.0) * spacing,
                        displacement,
                        (row as f32 - rows as f32 / 2.0) * spacing,
                        0.0,
                    ],
                    speed: [0.0, 0.0, 0.0, 0.0],
                }
            })
        })
        .collect();
    println!("Created {} instances", instances.len()); // Debug line

    let instances_copy = instances.clone();

    (vertices, index_buffer, instances, instances_copy, indices)  // Return indices as well
}


const WORKGROUP_SIZE: u32 = 64;
const GRID_SIZE: u32 = 64;

impl InstanceApp {
    pub fn new(context: &Context) -> Self {

        let (vertices, index_buffer, instances, instances_copy , indices) = generate_grid(
            &context,
            GRID_SIZE,          // rows
            GRID_SIZE,          // cols
            0.002,        // spacing (closer together for cloth-like appearance)
            1.0,         // displacement, where it starts on the y axis
            0.003,        // sphere_scale (smaller spheres to look like connection points)
            [0.1, 0.1, 0.1]    // color
        );
        

        let num_indices = indices.len() as u32;
        let num_instances = instances.len() as u32;

        let time_uniform = TimeUniform {
            generation_duration: Duration::new(0, 1_000_000).as_secs_f32(), // Use the generation_duration from the struct
        };



        // Beginning spring structure

        // let springs = generate_structural_springs(GRID_SIZE, 0.002);
        let springs = generate_springs(GRID_SIZE, 0.002);

        let spring_buffer = context.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Spring Buffer"),
            contents: bytemuck::cast_slice(&springs),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        });

        // End Spring structure

        let vertex_buffer =
            context
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Vertex Buffer"),
                    contents: bytemuck::cast_slice(vertices.as_slice()),
                    usage: wgpu::BufferUsages::VERTEX,
                });


        let instance_buffer = [
            context
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Instance Buffer Ping"),
                    contents: bytemuck::cast_slice(&instances.as_slice()),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX, // Add VERTEX usage
                }),
            context
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Instance Buffer Pong"),
                    contents: bytemuck::cast_slice(&instances_copy.as_slice()),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX, // Add VERTEX usage
                }),
        ];
        // Création de la sphère
        let (positions, indices) = icosphere(3);
        let sphere_radius = 0.3;

        let vertices: Vec<Vertex> = positions
            .iter()
            .map(|position| {
                let normal = position.normalize();
                Vertex {
                    position: (normal * sphere_radius).into(),
                    normal: normal.into(),
                    color: [0.8, 0.3, 0.3],
                }
            })
            .collect();

        let sphere_vertex_buffer = context
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Sphere Vertex Buffer"),
                contents: bytemuck::cast_slice(vertices.as_slice()),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let sphere_index_buffer = context
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Sphere Index Buffer"),
                contents: bytemuck::cast_slice(indices.as_slice()),
                usage: wgpu::BufferUsages::INDEX,
            });

        //Simulation parameters

        let simulation_parameters = [
            1.0,            // Mass of a particule
            1.0,            // Stiffness constant of spring 1 (Structural springs)
            1.1,            // Rest length of spring 1
            0.87,           // Stiffness constant of spring 2 (Shear springs)
            1.1,            // Rest length of spring 2
            3.14,           // Stiffness constant of spring 3 (Bend spring)
            1.1,            // Rest length of spring 3
            sphere_radius,  // Sphere Radius
            4.5,
        ];


        

        // Grid logic
        let shader = context
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
            });

        let compute_shader = context
        .device()
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(
            include_str!("compute.wgsl")
                .replace("WORKGROUP_SIZE", &format!("{}", WORKGROUP_SIZE))
                .into()
            ),
        });

        let camera_bind_group_layout = context
            .device()
            .create_bind_group_layout(&CameraUniform::desc());

        // Create simulation parameters buffer
        let sim_params = SimParams {
            delta_time: 0.001, // 60 FPS
            damping: 0.05,
            mass: 1.0,
            };
    
    
        let sim_params_buffer = context.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Simulation Parameters Buffer"),
            contents: bytemuck::cast_slice(&[sim_params]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });


        let instance_bind_group_layout = context.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Compute Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },

                // Uniform buffer for springs
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },

                // Uniform buffer for the simulation parameters
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },

                wgpu::BindGroupLayoutEntry {
                    binding: 4,  // New binding
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Create the debug buffer
        let debug_buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("Debug Buffer"),
            size: std::mem::size_of::<DebugData>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });


        let pipeline_layout =
            context
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Render Pipeline Layout"),
                    bind_group_layouts: &[&camera_bind_group_layout],
                    push_constant_ranges: &[],
                });

        let compute_pipeline_layout = 
            context
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Compute Pipeline Layout"),
                    bind_group_layouts: &[&instance_bind_group_layout],
                    push_constant_ranges: &[],
        });

        let render_pipeline =
            context
                .device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Render Pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: "vs_main",
                        buffers: &[Vertex::desc(), Instance::desc()],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: "fs_main",
                        targets: &[Some(wgpu::ColorTargetState {
                            format: context.format(),
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: Some(wgpu::Face::Back),
                        // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                        polygon_mode: wgpu::PolygonMode::Fill,
                        // Requires Features::DEPTH_CLIP_CONTROL
                        unclipped_depth: false,
                        // Requires Features::CONSERVATIVE_RASTERIZATION
                        conservative: false,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: context.depth_stencil_format(),
                        depth_write_enabled: true,
                        depth_compare: wgpu::CompareFunction::Less,
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview: None,
                    cache: None,
                });

        let aspect = context.size().x / context.size().y;
        let mut camera = OrbitCamera::new(context, 45.0, aspect, 0.1, 100.0);
        camera
            .set_polar(cgmath::point3(1.5, 0.0, 0.0))
            .update(context);

        let compute_pipeline =
        context
            .device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Compute Pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: "computeMain",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
            });
        

        let bind_group = [
            context
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Bind Group Ping"),
                    layout: &instance_bind_group_layout,
                    entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: instance_buffer[0].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: instance_buffer[1].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: spring_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: sim_params_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {  // Add this new entry
                        binding: 4,
                        resource: debug_buffer.as_entire_binding(),
                    }    
                    ],
                }),
            context
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Bind Group Pong"),
                layout: &instance_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                    binding: 0,
                    resource: instance_buffer[1].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                    binding: 1,
                    resource: instance_buffer[0].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                    binding: 2,
                    resource: spring_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                    binding: 3,
                    resource: sim_params_buffer.as_entire_binding(),
                    },

                    wgpu::BindGroupEntry {  // Add this new entry
                        binding: 4,
                        resource: debug_buffer.as_entire_binding(),
                    }
                    ],
                }),
            ];

        let sphere_shader = context
        .device()
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sphere Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sphere_shader.wgsl").into()),
        });
    
        let sphere_pipeline_layout = context
            .device()
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Sphere Pipeline Layout"),
                bind_group_layouts: &[&camera_bind_group_layout], // Use the same camera bind group
                push_constant_ranges: &[],
            });

        let sphere_render_pipeline = context
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Sphere Render Pipeline"),
                layout: Some(&sphere_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &sphere_shader,
                    entry_point: "vs_main",
                    buffers: &[Vertex::desc()], // Use the same vertex layout as the grid
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &sphere_shader,
                    entry_point: "fs_main",
                    targets: &[Some(wgpu::ColorTargetState {
                        format: context.format(),
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: context.depth_stencil_format(),
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            });
        Self {
            vertex_buffer,
            instance_buffer,
            index_buffer,
            render_pipeline,
            compute_pipeline,
            num_indices,
            num_instances,
            camera,
            generation_duration: Duration::from_micros(1_600), // 1.6ms
            last_generation: Instant::now(),
            bind_group,
            sphere_index_buffer,
            sphere_vertex_buffer,
            num_sphere_indices: indices.len() as u32,
            sphere_render_pipeline,
            debug_buffer,
        }
    }


}

impl App for InstanceApp {
    fn input(&mut self, input: egui::InputState, context: &Context) {
        self.camera.input(input, context);
    }
    
    fn update(&mut self, delta_time: f32, context: &Context) {
        if self.last_generation + self.generation_duration < Instant::now() {
            let mut encoder = context.device().create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Compute Encoder"),
            });

            {
                let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Compute Pass"),
                    timestamp_writes: None,
                });

                compute_pass.set_pipeline(&self.compute_pipeline);
                compute_pass.set_bind_group(0, &self.bind_group[0], &[]);
                compute_pass.dispatch_workgroups(self.num_instances / WORKGROUP_SIZE, 1, 1);
            }


            let debug_staging_buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("Debug Staging Buffer"),
                size: std::mem::size_of::<DebugData>() as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // Copy debug data
            encoder.copy_buffer_to_buffer(
                &self.debug_buffer,
                0,
                &debug_staging_buffer,
                0,
                std::mem::size_of::<DebugData>() as wgpu::BufferAddress,
            );

            context.queue().submit(std::iter::once(encoder.finish()));

            // After submitting command encoder:
            let debug_slice = debug_staging_buffer.slice(..);
            debug_slice.map_async(wgpu::MapMode::Read, |_| {});
            context.device().poll(wgpu::Maintain::Wait);

            let data = debug_slice.get_mapped_range();
            let debug_data: DebugData = bytemuck::cast_slice(&data)[0];
            println!("Forces on instance 0:");
            println!("  Gravity force: {}", debug_data.gravity_force);
            println!("  Total spring force: {}", debug_data.total_force);
            println!("  Final force (with damping): {}", debug_data.final_force);

            self.last_generation = Instant::now();

            // Swap the ping-pong buffers
            self.instance_buffer.swap(0, 1);
            self.bind_group.swap(0, 1);
        }
    }
    fn render(&self, render_pass: &mut wgpu::RenderPass<'_>) {

        render_pass.set_bind_group(0, self.camera.bind_group(), &[]);


        

        // Render the grid
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.instance_buffer[0].slice(..)); // Use the updated buffer
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.num_indices, 0, 0..self.num_instances);


        // Render the sphere
        render_pass.set_pipeline(&self.sphere_render_pipeline); // Use the sphere's pipeline
        render_pass.set_vertex_buffer(0, self.sphere_vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.sphere_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.num_sphere_indices, 0, 0..1);


        
    }
    
}

