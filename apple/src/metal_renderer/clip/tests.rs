use super::{CompositeParams, Prepared, Target};
use crate::metal_renderer::{
    InstanceBufferPool, InstanceBufferWriter, MetalRenderer, Viewport,
    new_command_encoder_for_texture, read_texture_to_image,
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
                let bindings = MetalRenderer::prepare_bindings(&scene, &mut writer).unwrap();
                let queue = renderer.command_queue.clone();
                let commands = queue.new_command_buffer();
                renderer
                    .draw_scene(
                        &scene,
                        &bindings,
                        &prepared.target,
                        &[prepared.target.bounds()],
                        commands,
                        &mut 0,
                        Some(0.),
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

#[cfg(feature = "test-support")]
#[test]
#[ignore = "requires a macOS Metal device"]
fn tiled_edges_match_full_surface_with_paths_and_fractional_origins() {
    use gpui::{PathBuilder, px, test::with_scene_clip};
    objc2::rc::autoreleasepool(|_| {
        let pool = Arc::new(Mutex::new(InstanceBufferPool::default()));
        let mut renderer = MetalRenderer::new_headless(pool.clone());
        renderer.opaque = false;
        let viewport = size(DevicePixels(128), DevicePixels(128));
        for scale in [1., 1.25, 2.] {
            for offset in [0., 0.3, 0.75] {
                let bounds = Bounds::new(
                    point(
                        ScaledPixels(9. * scale + offset),
                        ScaledPixels(13. * scale + offset),
                    ),
                    size(ScaledPixels(43. * scale), ScaledPixels(47. * scale)),
                );
                let mask = ContentMask { bounds };
                let radii = Corners {
                    top_left: ScaledPixels(8. * scale),
                    top_right: ScaledPixels(0.),
                    bottom_left: ScaledPixels(4. * scale),
                    bottom_right: ScaledPixels(12. * scale),
                };
                let mut scene = Scene::default();
                with_scene_clip(&mut scene, "clip", bounds, radii, mask, |s| {
                    for color in [
                        rgb(0xff0000).into(),
                        gpui::Hsla {
                            h: 0.6,
                            s: 1.,
                            l: 0.5,
                            a: 0.6,
                        },
                    ] {
                        s.insert_primitive(Quad {
                            bounds,
                            content_mask: mask,
                            background: color.into(),
                            ..Default::default()
                        });
                    }
                    let mut path = PathBuilder::fill();
                    path.move_to(point(px(0.), px(0.)));
                    path.line_to(point(px(110.), px(85.)));
                    path.line_to(point(px(0.), px(128.)));
                    path.close();
                    let mut path = path.build().unwrap().scale(1.);
                    path.color = rgb(0x00ff00).into();
                    path.content_mask = mask;
                    s.insert_primitive(path);
                });
                scene.finish();
                let actual = renderer.render_scene_to_image(&scene, viewport).unwrap();
                let composite = &scene.composites[0];
                let allocation = Bounds::new(
                    point(
                        ScaledPixels(bounds.left().0.floor()),
                        ScaledPixels(bounds.top().0.floor()),
                    ),
                    size(
                        ScaledPixels(bounds.right().0.ceil() - bounds.left().0.floor()),
                        ScaledPixels(bounds.bottom().0.ceil() - bounds.top().0.floor()),
                    ),
                );
                let prepared = renderer.prepare_composite(composite, allocation, 0);
                let descriptor = metal::TextureDescriptor::new();
                descriptor.set_width(128);
                descriptor.set_height(128);
                descriptor.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
                descriptor.set_storage_mode(MTLStorageMode::Managed);
                descriptor.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
                let destination = renderer.device.new_texture(&descriptor);
                let mut writer =
                    InstanceBufferWriter::new(&renderer.device, &pool, renderer.is_unified_memory);
                let bindings =
                    MetalRenderer::prepare_bindings(composite.scene(), &mut writer).unwrap();
                let queue = renderer.command_queue.clone();
                let commands = queue.new_command_buffer();
                renderer
                    .draw_scene(
                        composite.scene(),
                        &bindings,
                        &prepared.target,
                        &[allocation],
                        commands,
                        &mut 1,
                        Some(0.),
                    )
                    .unwrap();
                let encoder = new_command_encoder_for_texture(
                    commands,
                    &destination,
                    Viewport {
                        size: viewport,
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
                let expected = read_texture_to_image(&destination).unwrap();
                pool.lock().release(buffer);
                let worst = actual
                    .as_raw()
                    .iter()
                    .zip(expected.as_raw())
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(
                    worst <= 1,
                    "tile seams at scale={scale}, offset={offset}: max difference {worst}"
                );
            }
        }
    });
}
