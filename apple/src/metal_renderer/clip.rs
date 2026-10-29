use super::{InstanceBindings, InstanceBufferWriter, MetalRenderer, Viewport, write_instances};
use anyhow::Result;
use gpui::{Bounds, Composite, Corners, DevicePixels, ScaledPixels, point, size};
use metal::{MTLPixelFormat, MTLStorageMode, MTLTextureType, MTLTextureUsage};
use std::mem;

#[derive(Clone)]
pub(super) struct Target {
    pub texture: metal::Texture,
    pub paths: Option<metal::Texture>,
    pub path_msaa: Option<metal::Texture>,
    pub viewport: Viewport,
}

pub(super) struct Surface {
    pub target: Target,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct CompositeParams {
    pub bounds: Bounds<ScaledPixels>,
    pub clip_bounds: Bounds<ScaledPixels>,
    pub radii: Corners<ScaledPixels>,
}

pub(super) struct Prepared {
    pub target: Target,
    pub params: CompositeParams,
}

impl MetalRenderer {
    pub(super) fn prepare_composite(
        &mut self,
        composite: &Composite,
        parent: &Target,
        ix: usize,
    ) -> Option<Prepared> {
        let viewport = parent.viewport;
        let parent_bounds = Bounds::new(
            viewport.origin,
            size(
                ScaledPixels(viewport.size.width.0 as f32),
                ScaledPixels(viewport.size.height.0 as f32),
            ),
        );
        let visible = composite
            .bounds()
            .intersect(&composite.content_mask().bounds)
            .intersect(&parent_bounds);
        if visible.is_empty() {
            return None;
        }
        let origin = point(
            ScaledPixels(visible.left().0.floor()),
            ScaledPixels(visible.top().0.floor()),
        );
        let width = (visible.right().0.ceil() - origin.x.0) as u64;
        let height = (visible.bottom().0.ceil() - origin.y.0) as u64;
        while self.clip_surfaces.len() <= ix {
            self.clip_surfaces.push(None);
        }
        if self.clip_surfaces[ix].as_ref().is_none_or(|surface| {
            surface.target.texture.width() != width || surface.target.texture.height() != height
        }) {
            self.clip_surfaces[ix] = None;
            let descriptor = metal::TextureDescriptor::new();
            descriptor.set_texture_type(MTLTextureType::D2);
            descriptor.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            descriptor.set_width(width);
            descriptor.set_height(height);
            descriptor.set_storage_mode(MTLStorageMode::Private);
            descriptor.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self.clip_surfaces[ix] = Some(Surface {
                target: Target {
                    texture: self.device.new_texture(&descriptor),
                    paths: None,
                    path_msaa: None,
                    viewport: Viewport {
                        size: size(DevicePixels(width as i32), DevicePixels(height as i32)),
                        origin,
                    },
                },
            });
        }
        let target = &mut self.clip_surfaces[ix]
            .as_mut()
            .expect("surface prepared")
            .target;
        target.viewport.origin = origin;
        if !composite.scene().paths.is_empty() && target.paths.is_none() {
            let descriptor = metal::TextureDescriptor::new();
            descriptor.set_texture_type(MTLTextureType::D2);
            descriptor.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            descriptor.set_width(width);
            descriptor.set_height(height);
            descriptor.set_storage_mode(MTLStorageMode::Private);
            descriptor.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            target.paths = Some(self.device.new_texture(&descriptor));
            if self.path_sample_count > 1 {
                descriptor.set_texture_type(MTLTextureType::D2Multisample);
                descriptor.set_storage_mode(if self.is_apple_gpu {
                    MTLStorageMode::Memoryless
                } else {
                    MTLStorageMode::Private
                });
                descriptor.set_sample_count(self.path_sample_count as u64);
                descriptor.set_usage(MTLTextureUsage::RenderTarget);
                target.path_msaa = Some(self.device.new_texture(&descriptor));
            }
        }
        if composite.scene().paths.is_empty() {
            target.paths = None;
            target.path_msaa = None;
        }
        Some(Prepared {
            target: target.clone(),
            params: CompositeParams {
                bounds: Bounds::new(
                    origin,
                    size(ScaledPixels(width as f32), ScaledPixels(height as f32)),
                ),
                clip_bounds: composite.bounds(),
                radii: composite.radii(),
            },
        })
    }

    pub(super) fn draw_composite(
        &self,
        prepared: &Prepared,
        encoder: &metal::RenderCommandEncoderRef,
    ) {
        encoder.set_render_pipeline_state(&self.composite_pipeline_state);
        encoder.set_vertex_buffer(0, Some(&self.unit_vertices), 0);
        encoder.set_vertex_bytes(
            1,
            mem::size_of::<CompositeParams>() as u64,
            &prepared.params as *const _ as *const _,
        );
        encoder.set_fragment_bytes(
            1,
            mem::size_of::<CompositeParams>() as u64,
            &prepared.params as *const _ as *const _,
        );
        encoder.set_fragment_texture(0, Some(&prepared.target.texture));
        encoder.draw_primitives(metal::MTLPrimitiveType::Triangle, 0, 6);
    }

    pub(super) fn prepare_children(
        &mut self,
        scene: &gpui::Scene,
        target: &Target,
        writer: &mut InstanceBufferWriter,
        command_buffer: &metal::CommandBufferRef,
        surface_ix: &mut usize,
    ) -> Result<Vec<Option<Prepared>>> {
        let mut result = Vec::with_capacity(scene.composites.len());
        for composite in &scene.composites {
            let prepared = self.prepare_composite(composite, target, *surface_ix);
            if let Some(prepared) = &prepared {
                *surface_ix += 1;
                let instances: InstanceBindings = write_instances(composite.scene(), writer)?;
                self.draw_scene(
                    composite.scene(),
                    &instances,
                    writer,
                    &prepared.target,
                    command_buffer,
                    surface_ix,
                    0.,
                )?;
            }
            result.push(prepared);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
