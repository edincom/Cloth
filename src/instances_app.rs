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
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
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
struct TimeUniform {
    generation_duration: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct PhysicsParams {
    structural_k: f32,
    shear_k: f32,
    bend_k: f32,
    damping: f32,
    mass: f32,
    rest_length: f32,
    dt: f32,
    //padding: f32,
    friction: f32,
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
    time_buffer: wgpu::Buffer,
    physics_buffer: wgpu::Buffer,
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
    let (positions, indices) = icosphere(2);

    let vertices: Vec<Vertex> = positions
        .iter()
        .map(|position| Vertex {
            position: (*position * sphere_scale).into(),
            normal: [0.0, 0.0, 0.0],
            color: sphere_color,
        })
        .collect();

    let index_buffer = context
        .device()
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(indices.as_slice()),
            usage: wgpu::BufferUsages::INDEX,
        });

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

    let instances_copy = instances.clone();

    (vertices, index_buffer, instances, instances_copy, indices)
}

const WORKGROUP_SIZE: u32 = 128;
const GRID_SIZE: u32 = 128;

impl InstanceApp {
    pub fn new(context: &Context) -> Self {
        let (vertices, index_buffer, instances, instances_copy, indices) = generate_grid(
            &context,
            GRID_SIZE,
            GRID_SIZE,
            0.006,
            1.0,
            0.003,
            [0.1, 0.1, 0.1]
        );

        let num_indices = indices.len() as u32;
        let num_instances = instances.len() as u32;

        let time_uniform = TimeUniform {
            generation_duration: Duration::new(0, 1_000_000).as_secs_f32(),
        };
        
        let time_buffer = context.device().create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Time Uniform Buffer"),
            contents: bytemuck::cast_slice(&[time_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let physics_params = PhysicsParams {
            structural_k: 7000.0,  
            shear_k: 3000.0,       
            bend_k: 6000.0,        
            damping: 5.0,         
            mass: 0.8,        
            rest_length: 0.006,
            dt: 0.005,
            //padding: 0.0,
            friction: 0.9,
        };
 

        let physics_buffer = context.device().create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Physics Params Buffer"),
                contents: bytemuck::cast_slice(&[physics_params]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );

        let vertex_buffer = context
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
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX,
                }),
            context
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Instance Buffer Pong"),
                    contents: bytemuck::cast_slice(&instances_copy.as_slice()),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX,
                }),
        ];

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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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
            ],
        });

        let pipeline_layout = context
            .device()
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let compute_pipeline_layout = context.device().create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Compute Pipeline Layout"),
            bind_group_layouts: &[&instance_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = context
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

        let aspect = context.size().x / context.size().y;
        let mut camera = OrbitCamera::new(context, 45.0, aspect, 0.1, 100.0);
        camera
            .set_polar(cgmath::point3(1.5, 0.0, 0.0))
            .update(context);

        let compute_pipeline = context
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
                            resource: time_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: physics_buffer.as_entire_binding(),
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
                            resource: time_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: physics_buffer.as_entire_binding(),
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
                bind_group_layouts: &[&camera_bind_group_layout],
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
                    buffers: &[Vertex::desc()],
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
            generation_duration: Duration::from_micros(1_600),
            last_generation: Instant::now(),
            bind_group,
            sphere_index_buffer,
            sphere_vertex_buffer,
            num_sphere_indices: indices.len() as u32,
            sphere_render_pipeline,
            time_buffer,
            physics_buffer,
        }
    }
}

impl App for InstanceApp {
    fn input(&mut self, input: egui::InputState, context: &Context) {
        self.camera.input(input, context);
    }
    
    fn update(&mut self, _delta_time: f32, context: &Context) {
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

            context.queue().submit(std::iter::once(encoder.finish()));
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
        render_pass.set_vertex_buffer(1, self.instance_buffer[0].slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.num_indices, 0, 0..self.num_instances);

        // Render the sphere
        render_pass.set_pipeline(&self.sphere_render_pipeline);
        render_pass.set_vertex_buffer(0, self.sphere_vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.sphere_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.num_sphere_indices, 0, 0..1);
    }
}