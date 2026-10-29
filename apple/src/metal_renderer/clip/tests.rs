use super::{CompositeParams, Prepared, Target};
use crate::metal_renderer::{
    InstanceBufferPool, InstanceBufferWriter, MetalRenderer, Viewport,
    new_command_encoder_for_texture, read_texture_to_image, write_instances,
};
use gpui::{
    Bounds, ContentMask, Corners, DevicePixels, Quad, ScaledPixels, Scene, point, rgb, size,
};
use metal::{MTLPixelFormat, MTLStorageMode, MTLTextureUsage};
use parking_lot::Mutex;
use std::sync::Arc;

#[test]
#[ignore = "requires a macOS Metal device"]
fn composited_edges_have_one_coverage_after_overlapping_backgrounds() {
    objc2::rc::autoreleasepool(|_| {
        let pool = Arc::new(Mutex::new(InstanceBufferPool::default()));
        let mut renderer = MetalRenderer::new_headless(pool.clone());
        for scale in [1.0_f32, 1.25, 2.] {
            let extent = (96. * scale) as i32;
            let origin = point(
                ScaledPixels((10. * scale).floor()),
                ScaledPixels((20. * scale).floor()),
            );
            let width = (74. * scale).ceil() - origin.x.0;
            let height = (84. * scale).ceil() - origin.y.0;
            let bounds = Bounds::new(origin, size(ScaledPixels(width), ScaledPixels(height)));
            let clip_bounds = Bounds::new(
                point(ScaledPixels(10. * scale), ScaledPixels(20. * scale)),
                size(ScaledPixels(64. * scale), ScaledPixels(64. * scale)),
            );
            let texture = |width, height| {
                let descriptor = metal::TextureDescriptor::new();
                descriptor.set_width(width);
                descriptor.set_height(height);
                descriptor.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
                descriptor.set_storage_mode(MTLStorageMode::Managed);
                descriptor.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
                renderer.device.new_texture(&descriptor)
            };
            let source = texture(width as u64, height as u64);
            let destination = texture(extent as u64, extent as u64);
            let prepared = Prepared {
                target: Target {
                    texture: source,
                    paths: None,
                    path_msaa: None,
                    viewport: Viewport {
                        size: size(DevicePixels(width as i32), DevicePixels(height as i32)),
                        origin,
                    },
                },
                params: CompositeParams {
                    bounds,
                    clip_bounds,
                    radii: Corners::all(ScaledPixels(16. * scale)),
                },
            };
            let mut outputs = Vec::new();
            for first_color in [1, 0] {
                let mut scene = Scene::default();
                for &color in &[0xff0000, 0x0000ff][first_color..] {
                    scene.insert_primitive(Quad {
                        bounds,
                        content_mask: ContentMask { bounds },
                        background: rgb(color).into(),
                        ..Default::default()
                    });
                }
                scene.finish();
                let mut writer =
                    InstanceBufferWriter::new(&renderer.device, &pool, renderer.is_unified_memory);
                let instances = write_instances(&scene, &mut writer).unwrap();
                let queue = renderer.command_queue.clone();
                let commands = queue.new_command_buffer();
                renderer
                    .draw_scene(
                        &scene,
                        &instances,
                        &mut writer,
                        &prepared.target,
                        commands,
                        &mut 0,
                        0.,
                    )
                    .unwrap();
                let encoder = new_command_encoder_for_texture(
                    commands,
                    &destination,
                    Viewport {
                        size: size(DevicePixels(extent), DevicePixels(extent)),
                        origin: Default::default(),
                    },
                    Some(metal::MTLClearColor::new(0., 0., 0., 0.)),
                );
                renderer.draw_composite(&prepared, encoder);
                encoder.end_encoding();
                if !renderer.is_unified_memory {
                    let blit = commands.new_blit_command_encoder();
                    blit.synchronize_resource(&destination);
                    blit.end_encoding();
                }
                let buffer = writer.finish();
                commands.commit();
                commands.wait_until_completed();
                let pixels = read_texture_to_image(&destination).unwrap();
                pool.lock().release(buffer);
                assert_eq!(
                    pixels
                        .get_pixel((40. * scale) as u32, (50. * scale) as u32)
                        .0,
                    [0, 0, 255, 255]
                );
                assert_eq!(
                    pixels
                        .get_pixel((10. * scale) as u32, (20. * scale) as u32)
                        .0,
                    [0; 4]
                );
                assert!(pixels.pixels().any(|p| p[3] > 0 && p[3] < 255));
                assert!(
                    pixels
                        .pixels()
                        .all(|p| p[0] == 0 && p[1] == 0 && p[2] == p[3])
                );
                outputs.push(pixels);
            }
            assert_eq!(
                outputs[0], outputs[1],
                "overlapping backgrounds must not accumulate edge coverage at scale {scale}"
            );
        }
    });
}
