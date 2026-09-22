use std::borrow::Cow;
use std::mem::size_of;

use crate::graphics::QUERY_SHADER;
use crate::graphics::geometry::{GpuHit, GpuQuery, Hit, Query};
use crate::graphics::gpu::Gpu;
use crate::graphics::reflect::OBJECTS_BINDING;

const HEADER: u64 = 16;
const WORKGROUP: u32 = 64;

pub struct QueryState {
    pipeline: wgpu::ComputePipeline,
    objects_layout: wgpu::BindGroupLayout,
    query_layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    hits: Option<(wgpu::Buffer, wgpu::Buffer, u64)>,
}

impl QueryState {
    pub fn new(device: &wgpu::Device) -> QueryState {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("renderable queries"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(QUERY_SHADER)),
        });
        let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let objects_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("query objects"),
            entries: &[storage(OBJECTS_BINDING, true)],
        });
        let query_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("query"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage(1, false),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("renderable queries"),
            bind_group_layouts: &[Some(&objects_layout), Some(&query_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("renderable queries"),
            layout: Some(&layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("query"),
            size: size_of::<GpuQuery>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        QueryState {
            pipeline,
            objects_layout,
            query_layout,
            uniform,
            hits: None,
        }
    }

    fn hits(&mut self, device: &wgpu::Device, count: u64) -> (wgpu::Buffer, wgpu::Buffer) {
        let needed = HEADER + count.max(1) * size_of::<GpuHit>() as u64;
        match &self.hits {
            Some((buffer, staging, size)) if *size >= needed => (buffer.clone(), staging.clone()),
            _ => {
                let size = needed.next_power_of_two();
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("query hits"),
                    size,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                });
                let staging = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("query readback"),
                    size,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.hits = Some((buffer.clone(), staging.clone(), size));
                (buffer, staging)
            }
        }
    }

    pub fn run(&mut self, gpu: &Gpu, objects: &wgpu::Buffer, count: u32, query: &Query) -> Result<Vec<Hit>, String> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let device = &gpu.device;
        let (hits, staging) = self.hits(device, u64::from(count));
        gpu.queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&GpuQuery::new(query, count)));
        gpu.queue.write_buffer(&hits, 0, &[0; HEADER as usize]);

        let objects_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("query objects"),
            layout: &self.objects_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: OBJECTS_BINDING,
                resource: objects.as_entire_binding(),
            }],
        });
        let query_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("query"),
            layout: &self.query_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: hits.as_entire_binding(),
                },
            ],
        });

        let used = HEADER + u64::from(count) * size_of::<GpuHit>() as u64;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("query") });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("query"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &objects_group, &[]);
            pass.set_bind_group(1, &query_group, &[]);
            pass.dispatch_workgroups(count.div_ceil(WORKGROUP), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&hits, 0, &staging, 0, used);
        {
            let _queue = gpu.lock_queue();
            gpu.queue.submit([encoder.finish()]);
        }

        let slice = staging.slice(0..used);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| format!("the GPU query did not finish: {error}"))?;
        receiver
            .recv()
            .map_err(|_| "the GPU query was abandoned".to_owned())?
            .map_err(|error| format!("cannot read the GPU query results: {error}"))?;
        let found = {
            let data = slice
                .get_mapped_range()
                .map_err(|error| format!("cannot read the GPU query results: {error}"))?;
            let total = u32::from_le_bytes([data[0], data[1], data[2], data[3]]).min(count) as usize;
            data[HEADER as usize..used as usize]
                .chunks_exact(size_of::<GpuHit>())
                .take(total)
                .map(|chunk| bytemuck::pod_read_unaligned::<GpuHit>(chunk).hit())
                .collect()
        };
        staging.unmap();
        Ok(found)
    }
}
