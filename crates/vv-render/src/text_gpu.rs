//! The GPU side of the titles. A mask is uploaded once and its texture kept
//! while the mask lives; the sprites of an animated title are moved, faded,
//! revealed and blurred into a coverage target here (`text_parts.wgsl`), so
//! a frame of animation costs no rasterization and no upload.

use crate::text::TextMask;
use crate::text_anim_render::{Part, blur_pad, blur_step, dest_box};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use vv_core::RevealDir;
use wgpu::util::DeviceExt;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// Sides of the targets are multiples of this, so the sizes of an animation
/// repeat and the pool serves them.
const TARGET_GRID: u32 = 256;

/// Past that, the targets of sizes no longer used are let go.
const MAX_POOLED: usize = 24;

/// Taps per side the blur shader goes up to.
const MAX_BLUR_RADIUS: f32 = 160.0;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    quad: [f32; 4],
    matrix: [f32; 4],
    pivot_shift: [f32; 4],
    dest: [f32; 4],
    misc: [f32; 4],
    ink: [f32; 4],
    ramp_a: [f32; 4],
    ramp_b: [f32; 4],
}

impl Instance {
    /// A texture covering `quad` of the frame, drawn where it is.
    fn still(quad: [f32; 4], step: u32, alpha: f32, dest: [f32; 4]) -> Self {
        Self {
            quad,
            matrix: [1.0, 0.0, 0.0, 1.0],
            pivot_shift: [0.0; 4],
            dest,
            misc: [alpha, step as f32, -1.0, 0.0],
            ink: [0.0, 0.0, 1.0, 1.0],
            ramp_a: [0.0; 4],
            ramp_b: [0.0; 4],
        }
    }

    fn of(part: &Part, alpha: f32, dest: [f32; 4]) -> Self {
        let (x0, y0, x1, y1) = part.src.frame_box();
        let xf = &part.xf;
        let (sin, cos) = xf.rotation.sin_cos();
        // As small as the CPU path lets a sprite get.
        let (sx, sy) = (xf.scale[0].max(1e-3), xf.scale[1].max(1e-3));
        let mut instance = Self {
            matrix: [sx * cos, -sy * sin, sx * sin, sy * cos],
            pivot_shift: [xf.pivot.0, xf.pivot.1, xf.shift.0, xf.shift.1],
            ..Self::still([x0, y0, x1, y1], part.src.step, alpha, dest)
        };
        if let Some(reveal) = &part.reveal {
            let direction = match reveal.dir {
                RevealDir::Right => 0.0,
                RevealDir::Left => 1.0,
                RevealDir::Down => 2.0,
                RevealDir::Up => 3.0,
            };
            instance.misc[2] = reveal.progress;
            instance.misc[3] = direction;
            let ink = reveal.ink;
            instance.ink = [ink.0, ink.1, ink.2 - ink.0, ink.3 - ink.1];
        }
        if let Some(ramp) = &part.ramp {
            instance.ramp_a = [ramp.centre.0, ramp.centre.1, ramp.dir.0, ramp.dir.1];
            let kind = if ramp.radial { 2.0 } else { 1.0 };
            instance.ramp_b = [kind, ramp.offset, ramp.prev, ramp.inv];
        }
        instance
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurParams {
    direction: [f32; 2],
    sigma: f32,
    radius: f32,
}

struct Uploaded {
    /// Keeps the address of the mask from being reused while it is the key.
    owner: Weak<TextMask>,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

/// Parts blurred alike, drawn together into one target before the blur.
struct BlurGroup<'a> {
    sigma: f32,
    alpha: f32,
    parts: Vec<&'a Part>,
}

/// A blur group laid out: its area of the frame and the targets it uses.
struct Blurred {
    first: u32,
    /// The instance drawing the blurred target into the layer.
    composite: u32,
    binds: Vec<wgpu::BindGroup>,
    views: [wgpu::TextureView; 2],
    /// Texels of the targets the blur covers.
    used: (u32, u32),
    params: [wgpu::BindGroup; 2],
    target_bind: wgpu::BindGroup,
    /// Where it lands in the layer's target: x, y, width, height.
    scissor: [u32; 4],
}

pub(crate) struct TextGpu {
    parts: wgpu::RenderPipeline,
    parts_layout: wgpu::BindGroupLayout,
    blur: wgpu::RenderPipeline,
    blur_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    max_side: u32,
    masks: Mutex<HashMap<usize, Uploaded>>,
    targets: Mutex<Vec<wgpu::Texture>>,
}

/// A layer of parts drawn: the view of its target and the uv rectangle
/// (x0, y0, x1, y1) of the frame the target covers.
pub(crate) struct Coverage {
    pub view: wgpu::TextureView,
    pub rect: [f32; 4],
}

impl TextGpu {
    pub fn new(device: &wgpu::Device) -> Self {
        let parts_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vv-render text parts shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/text_parts.wgsl").into()),
        });
        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vv-render text blur shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/text_blur.wgsl").into()),
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4,
            4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4,
        ];
        // "Over" between coverages: a + b·(1 - a).
        let over = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrc,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = |label, shader: &wgpu::ShaderModule, buffers: &[_], blend, topology| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: None,
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    buffers,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: FORMAT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let parts = pipeline(
            "vv-render text parts pipeline",
            &parts_shader,
            &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Instance>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &attributes,
            })],
            Some(wgpu::BlendState {
                color: over,
                alpha: over,
            }),
            wgpu::PrimitiveTopology::TriangleStrip,
        );
        let blur = pipeline(
            "vv-render text blur pipeline",
            &blur_shader,
            &[],
            None,
            wgpu::PrimitiveTopology::TriangleList,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vv-render text sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            parts_layout: parts.get_bind_group_layout(0),
            blur_layout: blur.get_bind_group_layout(0),
            parts,
            blur,
            sampler,
            max_side: device.limits().max_texture_dimension_2d,
            masks: Mutex::default(),
            targets: Mutex::default(),
        }
    }

    fn sampled(&self, device: &wgpu::Device, view: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vv-render text mask bind group"),
            layout: &self.parts_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// The texture of `mask`, uploaded the first time it is asked for.
    fn uploaded(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mask: &Arc<TextMask>,
    ) -> (wgpu::TextureView, wgpu::BindGroup) {
        let key = Arc::as_ptr(mask) as usize;
        let mut masks = self.masks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(found) = masks.get(&key)
            && found.owner.strong_count() > 0
        {
            return (found.view.clone(), found.bind_group.clone());
        }
        let size = wgpu::Extent3d {
            width: mask.width.max(1),
            height: mask.height.max(1),
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vv-render text mask"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            &mask.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size.width),
                rows_per_image: Some(size.height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.sampled(device, &view);
        masks.insert(
            key,
            Uploaded {
                owner: Arc::downgrade(mask),
                view: view.clone(),
                bind_group: bind_group.clone(),
            },
        );
        (view, bind_group)
    }

    pub fn mask_view(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mask: &Arc<TextMask>,
    ) -> wgpu::TextureView {
        self.uploaded(device, queue, mask).0
    }

    /// A target of `size`, to give back with `recycle` once the commands
    /// drawing into it are submitted.
    fn target(
        &self,
        device: &wgpu::Device,
        size: (u32, u32),
        used: &mut Vec<wgpu::Texture>,
    ) -> wgpu::TextureView {
        let pooled = {
            let mut pool = self.targets.lock().unwrap_or_else(|e| e.into_inner());
            pool.iter()
                .position(|t| t.width() == size.0 && t.height() == size.1)
                .map(|i| pool.swap_remove(i))
        };
        let texture = pooled.unwrap_or_else(|| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vv-render text target"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        used.push(texture);
        view
    }

    /// The targets of a submitted frame go back to the pool, and the
    /// textures of the masks nobody holds any more are let go.
    pub fn recycle(&self, used: Vec<wgpu::Texture>) {
        if !used.is_empty() {
            let mut pool = self.targets.lock().unwrap_or_else(|e| e.into_inner());
            pool.extend(used);
            let excess = pool.len().saturating_sub(MAX_POOLED);
            pool.drain(..excess);
        }
        self.masks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, uploaded| uploaded.owner.strong_count() > 0);
    }

    fn grid(&self, len: u32) -> u32 {
        len.max(1).div_ceil(TARGET_GRID) * TARGET_GRID
    }

    fn begin<'e>(
        encoder: &'e mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        load: wgpu::LoadOp<wgpu::Color>,
    ) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vv-render text pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// Records the passes drawing `parts` (a layer of a title over a frame
    /// of `frame`) into a target; `None` if nothing of them shows.
    pub fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        parts: &[Part],
        frame: (u32, u32),
        used: &mut Vec<wgpu::Texture>,
    ) -> Option<Coverage> {
        let (frame_w, frame_h) = (frame.0 as i32, frame.1 as i32);
        let mut sharp: Vec<&Part> = Vec::new();
        let mut groups: Vec<BlurGroup> = Vec::new();
        for part in parts {
            if part.sigma < 0.5 {
                sharp.push(part);
                continue;
            }
            let same = |g: &&mut BlurGroup| g.sigma == part.sigma && g.alpha == part.alpha;
            match groups.iter_mut().find(same) {
                Some(group) => group.parts.push(part),
                None => groups.push(BlurGroup {
                    sigma: part.sigma,
                    alpha: part.alpha,
                    parts: vec![part],
                }),
            }
        }

        // What each group covers, cut to what can reach the frame.
        let union = |parts: &[&Part], pad: i32| -> Option<(i32, i32, i32, i32)> {
            let b = parts
                .iter()
                .filter_map(|p| dest_box(&p.src, &p.xf))
                .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))?;
            let b = (
                (b.0 - pad).max(-pad),
                (b.1 - pad).max(-pad),
                (b.2 + pad).min(frame_w + pad),
                (b.3 + pad).min(frame_h + pad),
            );
            (b.2 > b.0 && b.3 > b.1).then_some(b)
        };
        let in_frame = |b: (i32, i32, i32, i32)| -> Option<(i32, i32, i32, i32)> {
            let b = (b.0.max(0), b.1.max(0), b.2.min(frame_w), b.3.min(frame_h));
            (b.2 > b.0 && b.3 > b.1).then_some(b)
        };
        let group_areas: Vec<Option<(i32, i32, i32, i32)>> = groups
            .iter()
            .map(|g| union(&g.parts, blur_pad(g.sigma)).filter(|b| in_frame(*b).is_some()))
            .collect();
        let area = union(&sharp, 0)
            .into_iter()
            .chain(group_areas.iter().flatten().copied())
            .filter_map(in_frame)
            .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))?;
        let size = (
            self.grid((area.2 - area.0) as u32).min(self.max_side),
            self.grid((area.3 - area.1) as u32).min(self.max_side),
        );
        let dest = [area.0 as f32, area.1 as f32, size.0 as f32, size.1 as f32];

        let mut instances: Vec<Instance> = Vec::with_capacity(parts.len() + groups.len());
        let sharp_binds: Vec<wgpu::BindGroup> = sharp
            .iter()
            .map(|part| {
                instances.push(Instance::of(part, part.alpha, dest));
                self.uploaded(device, queue, &part.src.mask).1
            })
            .collect();
        let mut blurred: Vec<Blurred> = Vec::new();
        for (group, group_area) in groups.iter().zip(&group_areas) {
            let Some(b) = *group_area else { continue };
            let Some(shown) = in_frame(b) else { continue };
            let step = blur_step(group.sigma);
            let texels = |len: i32| (len as u32).div_ceil(step);
            let side = |len: i32| self.grid(texels(len)).min(self.max_side);
            let target = (side(b.2 - b.0), side(b.3 - b.1));
            let used_texels = (
                texels(b.2 - b.0).min(target.0),
                texels(b.3 - b.1).min(target.1),
            );
            let covered = [
                b.0 as f32,
                b.1 as f32,
                (target.0 * step) as f32,
                (target.1 * step) as f32,
            ];
            let first = instances.len() as u32;
            let binds = group
                .parts
                .iter()
                .map(|part| {
                    instances.push(Instance::of(part, 1.0, covered));
                    self.uploaded(device, queue, &part.src.mask).1
                })
                .collect();
            let composite = instances.len() as u32;
            instances.push(Instance::still(
                [
                    covered[0],
                    covered[1],
                    covered[0] + covered[2],
                    covered[1] + covered[3],
                ],
                step,
                group.alpha,
                dest,
            ));
            let views = [
                self.target(device, target, used),
                self.target(device, target, used),
            ];
            let sigma = group.sigma / step as f32;
            let params = [([1.0, 0.0], &views[0]), ([0.0, 1.0], &views[1])].map(
                |(direction, source)| {
                    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("vv-render text blur params"),
                        contents: bytemuck::bytes_of(&BlurParams {
                            direction,
                            sigma,
                            radius: (sigma * 3.0).ceil().min(MAX_BLUR_RADIUS),
                        }),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("vv-render text blur bind group"),
                        layout: &self.blur_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(source),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: buffer.as_entire_binding(),
                            },
                        ],
                    })
                },
            );
            blurred.push(Blurred {
                first,
                composite,
                binds,
                target_bind: self.sampled(device, &views[0]),
                views,
                used: used_texels,
                params,
                scissor: [
                    (shown.0 - area.0) as u32,
                    (shown.1 - area.1) as u32,
                    ((shown.2 - shown.0) as u32).min(size.0),
                    ((shown.3 - shown.1) as u32).min(size.1),
                ],
            });
        }

        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vv-render text instances"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let clear = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
        for group in &blurred {
            {
                let mut pass = Self::begin(encoder, &group.views[0], clear);
                pass.set_pipeline(&self.parts);
                pass.set_vertex_buffer(0, buffer.slice(..));
                for (i, bind) in group.binds.iter().enumerate() {
                    let instance = group.first + i as u32;
                    pass.set_bind_group(0, bind, &[]);
                    pass.draw(0..4, instance..instance + 1);
                }
            }
            // Rows into the second target, then columns back into the first.
            for (to, params, load) in [
                (1, &group.params[0], clear),
                (0, &group.params[1], wgpu::LoadOp::Load),
            ] {
                let mut pass = Self::begin(encoder, &group.views[to], load);
                pass.set_pipeline(&self.blur);
                pass.set_scissor_rect(0, 0, group.used.0, group.used.1);
                pass.set_bind_group(0, params, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        let view = self.target(device, size, used);
        let mut pass = Self::begin(encoder, &view, clear);
        pass.set_pipeline(&self.parts);
        pass.set_vertex_buffer(0, buffer.slice(..));
        for (i, bind) in sharp_binds.iter().enumerate() {
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..4, i as u32..i as u32 + 1);
        }
        for group in &blurred {
            let [x, y, w, h] = group.scissor;
            pass.set_scissor_rect(x, y, w, h);
            pass.set_bind_group(0, &group.target_bind, &[]);
            pass.draw(0..4, group.composite..group.composite + 1);
        }
        drop(pass);
        Some(Coverage {
            view,
            rect: [
                dest[0] / frame.0 as f32,
                dest[1] / frame.1 as f32,
                (dest[0] + dest[2]) / frame.0 as f32,
                (dest[1] + dest[3]) / frame.1 as f32,
            ],
        })
    }
}

#[cfg(test)]
#[path = "tests/text_gpu.rs"]
mod tests;
