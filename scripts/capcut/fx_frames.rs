// Offline renderer for asset shaders: copy to crates/vv-render/examples/
// while porting (do not commit it there), then
//   cargo build --release -p vv-render --example fx_frames
// usage: fx_frames <assets dir> <asset id> <a.rgba> <b.rgba> <w> <h> <out dir|-> <frames> <p0> <p1>
// The assets dir has the user layout (transitions/<name>/...), read at
// startup, so shaders can be edited without rebuilding. Out dir "-" only
// times the frames (worst one excludes the first, which compiles).
// Optional: CAPCUT_NATIVE=1000x646 and CAPCUT_BACKGROUND=0,171,59.
use vv_render::{Compositor, CustomPass, Layer, LayerContent, OutputFrame, assets, wgpu};

fn upload(c: &Compositor, path: &str, w: u32, h: u32) -> wgpu::Texture {
    let data = std::fs::read(path).unwrap();
    let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let tex = c.device().create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    c.queue().write_texture(
        tex.as_image_copy(),
        &data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
        size,
    );
    tex
}

fn layer(t: &wgpu::Texture, w: u32, h: u32) -> [Layer<'_>; 1] {
    [Layer::new(LayerContent::Texture { texture: t, source_size: (w, h) }, Default::default())]
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    assets::set_user_dir(Some(a[1].clone().into()));
    let lib = assets::reload();
    let asset = lib.get(&a[2]).expect("asset");
    let (w, h): (u32, u32) = (a[5].parse().unwrap(), a[6].parse().unwrap());
    let frames: u32 = a[8].parse().unwrap();
    let (p0, p1): (f32, f32) = (a[9].parse().unwrap(), a[10].parse().unwrap());
    let c = Compositor::new_headless();
    let mut params = asset.shader_params(&asset.default_params());
    if let Ok(native) = std::env::var("CAPCUT_NATIVE") {
        let (nw, nh) = native.split_once('x').expect("native WxH");
        let (nw, nh): (f32, f32) = (nw.parse().unwrap(), nh.parse().unwrap());
        let fit = (w as f32 / nw).min(h as f32 / nh);
        params = [0.5, 0.5, nw * fit / w as f32, nh * fit / h as f32, 0.0, 0.0, 0.0, 0.0];
    }
    let background = std::env::var("CAPCUT_BACKGROUND").ok().map(|v| {
        let values: Vec<f32> = v.split(',').map(|v| v.parse::<f32>().unwrap() / 255.0).collect();
        vv_core::Rgba { r: values[0], g: values[1], b: values[2], a: 1.0 }
    });
    let out = OutputFrame::exact(w, h);
    let ta = upload(&c, &a[3], w, h);
    let tb = upload(&c, &a[4], w, h);
    let la = c.render_layers_to_owned_texture_transparent(&layer(&ta, w, h), out);
    let lb = c.render_layers_to_owned_texture_transparent(&layer(&tb, w, h), out);
    let t0 = std::time::Instant::now();
    let mut worst = (std::time::Duration::ZERO, 0.0f32);
    for i in 0..frames {
        let tf = std::time::Instant::now();
        let p = p0 + (p1 - p0) * i as f32 / (frames - 1).max(1) as f32;
        let pass = CustomPass {
            key: &asset.id,
            source: &asset.shader,
            params,
            curves: asset.curve_values(p),
            progress: p,
            time: p * asset.default_secs as f32,
            amount: 1.0,
        };
        let r = c.render_custom_pass(&pass, &la, &lb).expect("compile");
        if a[7] == "-" {
            c.device().poll(wgpu::PollType::wait_indefinitely()).unwrap();
            if i > 0 && tf.elapsed() > worst.0 {
                worst = (tf.elapsed(), p);
            }
            continue;
        }
        let mut shown = Vec::new();
        if let Some(color) = background {
            shown.push(Layer::new(LayerContent::Solid(color), Default::default()));
        }
        shown.extend(layer(&r, w, h));
        let px = c.render_layers(&shown, out);
        std::fs::write(format!("{}/{i:03}.rgba", a[7]), px).unwrap();
    }
    eprintln!("{frames} frames in {:?}, worst {:?} at {:.3}", t0.elapsed(), worst.0, worst.1);
}
