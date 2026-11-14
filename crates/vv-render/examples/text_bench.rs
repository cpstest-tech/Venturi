//! Frame times of the bundled text templates played at full resolution.

use std::time::{Duration, Instant};
use vv_core::text_template::TextTemplateDef;
use vv_render::{Compositor, Layer, LayerContent, OutputFrame};

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../vv-app/assets");
    for entry in std::fs::read_dir(root.join("fonts")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "ttf") {
            vv_render::text::register_font(std::fs::read(path).unwrap());
        }
    }
    let args: Vec<String> = std::env::args().collect();
    let size: (u32, u32) = match args.get(1).map(String::as_str) {
        Some("4k") => (3840, 2160),
        Some("vertical") => (1080, 1920),
        Some("4kv") => (2160, 3840),
        _ => (1920, 1080),
    };
    let only = args.get(2).filter(|arg| !arg.starts_with("--"));
    let warm = args.iter().any(|arg| arg == "--warm");
    let snapshots = args.iter().any(|arg| arg == "--snapshots");
    let fps = 60.0;
    let compositor = Compositor::new_headless();
    let output = OutputFrame::exact(size.0, size.1);
    let mut names: Vec<_> = std::fs::read_dir(root.join("text-templates"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    names.sort();
    println!(
        "{:<22} {:>8} {:>8} {:>8} | {:>8} {:>8} {:>8} | {:>5}",
        "template", "cpu avg", "cpu p99", "cpu max", "tot avg", "tot p99", "tot max", ">8ms"
    );
    let mut worst = Duration::ZERO;
    for path in names {
        let def = TextTemplateDef::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
        if only.is_some_and(|o| *o != def.id) {
            continue;
        }
        let clips = def.clips(size);
        if snapshots {
            for (index, share) in [0.1, 0.5, 0.9].into_iter().enumerate() {
                let t = def.duration * share;
                let titles: Vec<_> = clips
                    .iter()
                    .filter(|c| t >= c.start_secs && t < c.start_secs + c.length_secs)
                    .map(|c| c.title.clone().animated(t - c.start_secs, c.length_secs))
                    .collect();
                let layers: Vec<_> = titles
                    .iter()
                    .map(|t| Layer::new(LayerContent::Text(t), Default::default()))
                    .collect();
                let pixels = compositor.render_layers_rgba_transparent(&layers, output);
                let mut ppm = format!("P6\n{} {}\n255\n", size.0, size.1).into_bytes();
                for pixel in pixels.as_chunks::<4>().0 {
                    ppm.extend_from_slice(&pixel[..3]);
                }
                std::fs::write(format!("/tmp/venturi-{}-{index}.ppm", def.id), ppm).unwrap();
            }
            continue;
        }
        if warm {
            for clip in &clips {
                vv_render::text::render_title(&clip.title, size, size);
                vv_render::text::render_title(
                    &clip.title.clone().animated(0.0, clip.length_secs),
                    size,
                    size,
                );
            }
        }
        let frames = (def.duration * fps) as usize;
        let (mut cpu, mut total) = (Vec::new(), Vec::new());
        for f in 0..frames {
            let t = f as f32 / fps;
            let titles: Vec<_> = clips
                .iter()
                .filter(|c| t >= c.start_secs && t < c.start_secs + c.length_secs)
                .map(|c| c.title.clone().animated(t - c.start_secs, c.length_secs))
                .collect();
            let start = Instant::now();
            for title in &titles {
                vv_render::text::render_title(title, size, size);
            }
            cpu.push(start.elapsed());
            let layers: Vec<Layer> = titles
                .iter()
                .map(|t| Layer::new(LayerContent::Text(t), Default::default()))
                .collect();
            let texture = compositor.render_layers_to_texture(&layers, output);
            compositor
                .device()
                .poll(vv_render::wgpu::PollType::wait_indefinitely())
                .unwrap();
            drop(texture);
            total.push(start.elapsed());
        }
        let stats = |v: &mut Vec<Duration>| {
            v.sort();
            let avg = v.iter().sum::<Duration>() / v.len().max(1) as u32;
            (avg, v[v.len() * 99 / 100], *v.last().unwrap())
        };
        let slow = total.iter().filter(|d| d.as_secs_f32() > 0.008).count();
        let (ca, cp, cm) = stats(&mut cpu);
        let (ta, tp, tm) = stats(&mut total);
        worst = worst.max(tm);
        let ms = |d: Duration| format!("{:.2}", d.as_secs_f64() * 1e3);
        println!(
            "{:<22} {:>8} {:>8} {:>8} | {:>8} {:>8} {:>8} | {:>5}",
            def.id,
            ms(ca),
            ms(cp),
            ms(cm),
            ms(ta),
            ms(tp),
            ms(tm),
            slow
        );
    }
    println!("worst frame: {:.2} ms", worst.as_secs_f64() * 1e3);
}
