use bevy::prelude::*;
use bevy::render::render_resource::{
    CommandEncoder, Texture, TextureDescriptor, TextureDimension, TextureUsages, TextureView,
    TextureViewDescriptor,
};
use bevy::render::renderer::RenderDevice;

/// The code texture `BLACK`: one texel, never written, so it reads as the
/// zero-initialised `(0, 0, 0, 1)` of an R8 image.
#[derive(Resource, Default)]
pub(super) struct CodeBlackImage {
    view: Option<TextureView>,
}

impl CodeBlackImage {
    pub fn ensure(&mut self, device: &RenderDevice) -> TextureView {
        self.view
            .get_or_insert_with(|| {
                device
                    .create_texture(&TextureDescriptor {
                        label: Some("exact_code_black"),
                        size: bevy::render::render_resource::Extent3d {
                            width: 1,
                            height: 1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format: bevy::render::render_resource::TextureFormat::R8Unorm,
                        usage: TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&TextureViewDescriptor::default())
            })
            .clone()
    }
}

#[derive(Resource, Default)]
pub(super) struct ResolvedScene {
    target: Option<Texture>,
    view: Option<TextureView>,
}

impl ResolvedScene {
    pub fn ensure(&mut self, device: &RenderDevice, source: &Texture) -> (TextureView, bool) {
        let format = source.format().remove_srgb_suffix();
        let changed = self
            .target
            .as_ref()
            .is_none_or(|t| t.size() != source.size() || t.format() != format);
        if changed {
            let texture = device.create_texture(&TextureDescriptor {
                label: Some("exact_resolved_post_sun"),
                size: source.size(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format,
                usage: TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.view = Some(texture.create_view(&TextureViewDescriptor::default()));
            self.target = Some(texture);
        }
        (
            self.view.as_ref().expect("ensure creates the view").clone(),
            changed,
        )
    }

    pub fn copy(&self, encoder: &mut CommandEncoder, source: &Texture) {
        let target = self.target.as_ref().expect("ensure before recording");
        encoder.copy_texture_to_texture(
            source.as_image_copy(),
            target.as_image_copy(),
            source.size(),
        );
    }
}
