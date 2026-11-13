//! Runs an asset shader (see `assets.rs`) as one fullscreen pass over one or
//! two composed textures. The shader file only defines
//! `fn effect(uv: vec2<f32>) -> vec4<f32>`; this prelude supplies the rest:
//!
//! - `src(uv)` / `dst(uv)`: input A / B, premultiplied RGBA (`dst` is the
//!   incoming clip of a transition, the same as `src` for effects);
//! - `fx_size()` pixels, `fx_progress()` 0..1, `fx_time()` seconds,
//!   `param(i)` for the manifest parameters;
//! - `straight(c)` / `premul(c)` to convert.
//!
//! The result is blended with `src` by the element's amount (opacity/fades).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const PRELUDE: &str = r#"
struct Fx {
    head: vec4<f32>,
    extra: vec4<f32>,
    p: array<vec4<f32>, 2>,
};
@group(0) @binding(0) var fx_sampler: sampler;
@group(0) @binding(1) var fx_a: texture_2d<f32>;
@group(0) @binding(2) var fx_b: texture_2d<f32>;
@group(0) @binding(3) var<uniform> fx: Fx;

fn src(uv: vec2<f32>) -> vec4<f32> { return textureSampleLevel(fx_a, fx_sampler, uv, 0.0); }
fn dst(uv: vec2<f32>) -> vec4<f32> { return textureSampleLevel(fx_b, fx_sampler, uv, 0.0); }
fn fx_size() -> vec2<f32> { return fx.head.xy; }
fn fx_progress() -> f32 { return fx.head.z; }
fn fx_time() -> f32 { return fx.head.w; }
fn param(i: u32) -> f32 { return fx.p[i / 4u][i % 4u]; }
fn straight(c: vec4<f32>) -> vec4<f32> {
    if (c.a <= 0.0001) { return vec4<f32>(0.0); }
    return vec4<f32>(c.rgb / c.a, c.a);
}
fn premul(c: vec4<f32>) -> vec4<f32> { return vec4<f32>(c.rgb * c.a, c.a); }

struct FxVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> FxVertex {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var out: FxVertex;
    out.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

@fragment
fn fs_main(in: FxVertex) -> @location(0) vec4<f32> {
    return mix(src(in.uv), effect(in.uv), clamp(fx.extra.x, 0.0, 1.0));
}
"#;

/// What one pass needs besides the textures.
pub struct CustomPass<'a> {
    /// Cache key of the compiled pipeline (the asset id).
    pub key: &'a str,
    pub source: &'a str,
    pub params: [f32; crate::assets::MAX_PARAMS],
    pub progress: f32,
    pub time: f32,
    /// 0..1: how much of the effect shows over the input.
    pub amount: f32,
}

impl CustomPass<'_> {
    pub(crate) fn uniform(&self, size: (u32, u32)) -> [f32; 16] {
        let mut u = [0.0; 16];
        u[..4].copy_from_slice(&[size.0 as f32, size.1 as f32, self.progress, self.time]);
        u[4] = self.amount;
        u[8..].copy_from_slice(&self.params);
        u
    }
}

pub(crate) struct CustomShaders {
    pub(crate) layout: wgpu::BindGroupLayout,
    /// `None`: it failed to compile, reported once and passed through after.
    pipelines: Mutex<HashMap<String, Option<Arc<wgpu::RenderPipeline>>>>,
}

impl CustomShaders {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vv-render custom bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture(1),
                texture(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        Self {
            layout,
            pipelines: Mutex::default(),
        }
    }

    pub(crate) fn clear(&self) {
        self.pipelines.lock().unwrap().clear();
    }

    pub(crate) fn pipeline(
        &self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        key: &str,
        source: &str,
    ) -> Option<Arc<wgpu::RenderPipeline>> {
        let mut cache = self.pipelines.lock().unwrap();
        if let Some(found) = cache.get(key) {
            return found.clone();
        }
        let built = match self.build(device, format, source) {
            Ok(pipeline) => Some(pipeline),
            Err(error) => {
                eprintln!("vv-render: shader of asset {key} does not compile: {error}");
                None
            }
        };
        cache.insert(key.to_owned(), built.clone());
        built
    }

    fn build(
        &self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        source: &str,
    ) -> Result<Arc<wgpu::RenderPipeline>, wgpu::Error> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vv-render custom shader"),
            source: wgpu::ShaderSource::Wgsl(format!("{PRELUDE}\n{source}").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vv-render custom pipeline layout"),
            bind_group_layouts: &[Some(&self.layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vv-render custom pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
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
        match pollster::block_on(scope.pop()) {
            None => Ok(Arc::new(pipeline)),
            Some(error) => Err(error),
        }
    }
}
