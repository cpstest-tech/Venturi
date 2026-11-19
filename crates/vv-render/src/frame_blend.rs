//! Motion blur on the GPU: frames summed with weights into a float buffer
//! (`shaders/frame_blend.wgsl`), and the difference between two frames to
//! spot duplicates.

use std::sync::Mutex;

use wgpu::util::DeviceExt;

const ACCUMULATE_WORKGROUP: u32 = 8;
const DIFFERENCE_WORKGROUP: u32 = 16;
/// Duplicates differ by nothing or by encoder noise: one luma sample every
/// few pixels measures that as well as all of them.
const DIFFERENCE_STRIDE: u32 = 4;
const FIXED_ONE: f32 = 1024.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlendParams {
    size: [u32; 2],
    weight: f32,
    gamma: f32,
    clear: u32,
    _pad: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DifferenceParams {
    grid: [u32; 2],
    stride: u32,
    _pad: u32,
}

/// Running weighted sum of frames of one size, from `Compositor::blend_sum`.
pub struct BlendSum {
    buffer: wgpu::Buffer,
    size: (u32, u32),
    gamma: f32,
    empty: bool,
}

impl BlendSum {
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Starts a new sum, keeping the buffer.
    pub fn reset(&mut self, gamma: f32) {
        self.gamma = gamma;
        self.empty = true;
    }
}

pub(crate) struct FrameBlend {
    accumulate: wgpu::ComputePipeline,
    difference: wgpu::ComputePipeline,
    module: wgpu::ShaderModule,
    /// One per target format.
    resolve: Mutex<Vec<(wgpu::TextureFormat, wgpu::RenderPipeline)>>,
    difference_total: wgpu::Buffer,
    difference_readback: wgpu::Buffer,
}

fn view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

impl FrameBlend {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vv-render frame blend shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/frame_blend.wgsl").into()),
        });
        let compute = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let buffer = |label: &str, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: 4,
                usage,
                mapped_at_creation: false,
            })
        };
        Self {
            accumulate: compute("accumulate"),
            difference: compute("difference"),
            resolve: Mutex::default(),
            difference_total: buffer(
                "vv-render frame difference total",
                wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
            ),
            difference_readback: buffer(
                "vv-render frame difference readback",
                wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            ),
            module,
        }
    }

    pub(crate) fn sum(&self, device: &wgpu::Device, size: (u32, u32), gamma: f32) -> BlendSum {
        BlendSum {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vv-render blend sum"),
                size: u64::from(size.0) * u64::from(size.1) * 16,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            }),
            size,
            gamma,
            empty: true,
        }
    }

    pub(crate) fn add(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sum: &mut BlendSum,
        frame: &wgpu::Texture,
        weight: f32,
    ) {
        debug_assert_eq!((frame.width(), frame.height()), sum.size);
        let params = BlendParams {
            size: [sum.size.0, sum.size.1],
            weight,
            gamma: sum.gamma,
            clear: u32::from(sum.empty),
            _pad: [0; 3],
        };
        sum.empty = false;
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vv-render blend params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let frame_view = view(frame);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vv-render blend accumulate bind group"),
            layout: &self.accumulate.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&frame_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: sum.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vv-render blend accumulate encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("vv-render blend accumulate pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.accumulate);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(
                sum.size.0.div_ceil(ACCUMULATE_WORKGROUP),
                sum.size.1.div_ceil(ACCUMULATE_WORKGROUP),
                1,
            );
        }
        queue.submit(Some(encoder.finish()));
    }

    fn resolve_pipeline(
        &self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let mut pipelines = self.resolve.lock().unwrap();
        if let Some((_, pipeline)) = pipelines.iter().find(|(f, _)| *f == format) {
            return pipeline.clone();
        }
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vv-render blend resolve pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &self.module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.module,
                entry_point: Some("fs_resolve"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        pipelines.push((format, pipeline.clone()));
        pipeline
    }

    /// Writes the sum into `out`, of its size. An empty sum leaves `out`
    /// transparent.
    pub(crate) fn resolve(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sum: &BlendSum,
        out: &wgpu::Texture,
    ) {
        let out_view = view(out);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vv-render blend resolve encoder"),
        });
        let pipeline = self.resolve_pipeline(device, out.format());
        let params = BlendParams {
            size: [sum.size.0, sum.size.1],
            weight: 1.0,
            gamma: sum.gamma,
            clear: 0,
            _pad: [0; 3],
        };
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vv-render blend resolve params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = (!sum.empty).then(|| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vv-render blend resolve bind group"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: sum.buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            })
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vv-render blend resolve pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &out_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(bind_group) = &bind_group {
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        queue.submit(Some(encoder.finish()));
    }

    /// Mean absolute luma difference, 0..1, between two frames of the same
    /// size. Blocks on the readback.
    pub(crate) fn difference(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        a: &wgpu::Texture,
        b: &wgpu::Texture,
    ) -> f32 {
        let grid = [
            a.width().div_ceil(DIFFERENCE_STRIDE),
            a.height().div_ceil(DIFFERENCE_STRIDE),
        ];
        let params = DifferenceParams {
            grid,
            stride: DIFFERENCE_STRIDE,
            _pad: 0,
        };
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vv-render frame difference params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let (a_view, b_view) = (view(a), view(b));
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vv-render frame difference bind group"),
            layout: &self.difference.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&a_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&b_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: self.difference_total.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        queue.write_buffer(&self.difference_total, 0, &[0; 4]);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vv-render frame difference encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("vv-render frame difference pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.difference);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(
                grid[0].div_ceil(DIFFERENCE_WORKGROUP),
                grid[1].div_ceil(DIFFERENCE_WORKGROUP),
                1,
            );
        }
        encoder.copy_buffer_to_buffer(&self.difference_total, 0, &self.difference_readback, 0, 4);
        queue.submit(Some(encoder.finish()));

        let slice = self.difference_readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("wgpu device poll failed");
        rx.recv()
            .expect("the map_async callback did not answer")
            .expect("map_async failed");
        let total = {
            let data = slice.get_mapped_range().expect("get_mapped_range failed");
            u32::from_le_bytes([data[0], data[1], data[2], data[3]])
        };
        self.difference_readback.unmap();
        let samples = f64::from(grid[0]) * f64::from(grid[1]);
        (f64::from(total) / f64::from(FIXED_ONE) / samples.max(1.0)) as f32
    }
}

#[cfg(test)]
#[path = "tests/frame_blend.rs"]
mod tests;
